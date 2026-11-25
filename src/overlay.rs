use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm, delegate_simple,
    globals::GlobalData,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState, SimpleGlobal},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{
            CursorIcon, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer,
        },
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};

use crate::capture::{self, Image};

const DIM: u32 = 140;
const BORDER: [u8; 4] = [0xe9, 0xae, 0x3d, 0xff];
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;

struct Drag {
    view: usize,
    start: (f64, f64),
    end: (f64, f64),
}

struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

struct View {
    image: Image,
    dimmed: Vec<u8>,
    layer: LayerSurface,
    viewport: WpViewport,
    pool: Option<SlotPool>,
    logical: (u32, u32),
    configured: bool,
    dirty: bool,
    frame_pending: bool,
}

struct App {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor: CompositorState,
    shm: Shm,
    viewporter: SimpleGlobal<WpViewporter, 1>,
    views: Vec<View>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<ThemedPointer>,
    drag: Option<Drag>,
    result: Option<Image>,
    done: bool,
}

pub fn run() -> Result<Option<Image>, String> {
    let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
    let (globals, mut queue): (_, EventQueue<App>) =
        registry_queue_init(&conn).map_err(|e| e.to_string())?;
    let qh = queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).map_err(|e| e.to_string())?;
    let layer_shell = LayerShell::bind(&globals, &qh).map_err(|e| e.to_string())?;
    let shm = Shm::bind(&globals, &qh).map_err(|e| e.to_string())?;
    let viewporter =
        SimpleGlobal::<WpViewporter, 1>::bind(&globals, &qh).map_err(|e| e.to_string())?;

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        compositor,
        shm,
        viewporter,
        views: Vec::new(),
        keyboard: None,
        pointer: None,
        drag: None,
        result: None,
        done: false,
    };
    queue.roundtrip(&mut app).map_err(|e| e.to_string())?;

    let outputs: Vec<(wl_output::WlOutput, String)> = app
        .output_state
        .outputs()
        .filter_map(|output| {
            let name = app.output_state.info(&output)?.name?;
            Some((output, name))
        })
        .collect();
    if outputs.is_empty() {
        return Err("no outputs found".into());
    }

    let names: Vec<String> = outputs.iter().map(|(_, name)| name.clone()).collect();
    let images = capture::capture_screens(&names)?;

    let viewporter = app.viewporter.get().map_err(|e| e.to_string())?.clone();
    for ((output, _), image) in outputs.into_iter().zip(images) {
        let surface = app.compositor.create_surface(&qh);
        let viewport = viewporter.get_viewport(&surface, &qh, GlobalData);
        let layer = layer_shell.create_layer_surface(
            &qh,
            surface,
            Layer::Overlay,
            Some("shoturscreen"),
            Some(&output),
        );
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer.commit();
        let dimmed = dim(&image.pixels);
        app.views.push(View {
            image,
            dimmed,
            layer,
            viewport,
            pool: None,
            logical: (0, 0),
            configured: false,
            dirty: true,
            frame_pending: false,
        });
    }

    while !app.done {
        queue
            .blocking_dispatch(&mut app)
            .map_err(|e| e.to_string())?;
    }

    for view in app.views.drain(..) {
        view.viewport.destroy();
        drop(view.layer);
    }
    let _ = queue.roundtrip(&mut app);
    Ok(app.result.take())
}

fn dim(pixels: &[u8]) -> Vec<u8> {
    pixels
        .chunks_exact(4)
        .flat_map(|px| {
            [
                (px[0] as u32 * DIM / 255) as u8,
                (px[1] as u32 * DIM / 255) as u8,
                (px[2] as u32 * DIM / 255) as u8,
                0xff,
            ]
        })
        .collect()
}

impl App {
    fn view_index(&self, surface: &wl_surface::WlSurface) -> Option<usize> {
        self.views
            .iter()
            .position(|view| view.layer.wl_surface() == surface)
    }

    fn redraw(&mut self, index: usize, qh: &QueueHandle<Self>) {
        self.views[index].dirty = true;
        self.draw(index, qh);
    }

    fn selection(&self) -> Option<(usize, Rect)> {
        let drag = self.drag.as_ref()?;
        let view = &self.views[drag.view];
        let (lw, lh) = (view.logical.0.max(1) as f64, view.logical.1.max(1) as f64);
        let sx = view.image.width as f64 / lw;
        let sy = view.image.height as f64 / lh;
        let clamp = |v: f64, max: f64| v.clamp(0.0, max);
        let x0 = (clamp(drag.start.0.min(drag.end.0), lw) * sx).round() as u32;
        let x1 = (clamp(drag.start.0.max(drag.end.0), lw) * sx).round() as u32;
        let y0 = (clamp(drag.start.1.min(drag.end.1), lh) * sy).round() as u32;
        let y1 = (clamp(drag.start.1.max(drag.end.1), lh) * sy).round() as u32;
        Some((
            drag.view,
            Rect {
                x: x0,
                y: y0,
                w: x1.min(view.image.width) - x0,
                h: y1.min(view.image.height) - y0,
            },
        ))
    }

    fn finish(&mut self) {
        if let Some((index, rect)) = self.selection().filter(|(_, r)| r.w > 0 && r.h > 0) {
            let image = &self.views[index].image;
            let stride = image.width as usize * 4;
            let mut pixels = Vec::with_capacity(rect.w as usize * rect.h as usize * 4);
            for y in rect.y..rect.y + rect.h {
                let start = y as usize * stride + rect.x as usize * 4;
                pixels.extend_from_slice(&image.pixels[start..start + rect.w as usize * 4]);
            }
            self.result = Some(Image {
                width: rect.w,
                height: rect.h,
                pixels,
            });
        }
        self.done = true;
    }

    fn draw(&mut self, index: usize, qh: &QueueHandle<Self>) {
        let selection = self
            .selection()
            .filter(|(view, _)| *view == index)
            .map(|(_, rect)| rect);
        let shm = &self.shm;
        let view = &mut self.views[index];
        if !view.configured || !view.dirty || view.frame_pending {
            return;
        }
        let width = view.image.width as i32;
        let height = view.image.height as i32;
        let stride = width * 4;
        let pool = match view.pool.as_mut() {
            Some(pool) => pool,
            None => match SlotPool::new((stride * height) as usize * 2, shm) {
                Ok(pool) => view.pool.insert(pool),
                Err(_) => return,
            },
        };
        let Ok((buffer, canvas)) =
            pool.create_buffer(width, height, stride, wl_shm::Format::Argb8888)
        else {
            return;
        };
        canvas.copy_from_slice(&view.dimmed);
        if let Some(rect) = selection {
            paint_selection(canvas, &view.image, &rect, view.logical.0);
        }

        let surface = view.layer.wl_surface();
        view.viewport
            .set_source(0.0, 0.0, width as f64, height as f64);
        view.viewport
            .set_destination(view.logical.0 as i32, view.logical.1 as i32);
        surface.damage_buffer(0, 0, width, height);
        surface.frame(qh, surface.clone());
        if buffer.attach_to(surface).is_err() {
            return;
        }
        view.layer.commit();
        view.dirty = false;
        view.frame_pending = true;
    }
}

fn paint_selection(canvas: &mut [u8], image: &Image, rect: &Rect, logical_width: u32) {
    if rect.w == 0 || rect.h == 0 {
        return;
    }
    let stride = image.width as usize * 4;
    let (x0, x1) = (rect.x as usize * 4, (rect.x + rect.w) as usize * 4);
    for y in rect.y..rect.y + rect.h {
        let row = y as usize * stride;
        canvas[row + x0..row + x1].copy_from_slice(&image.pixels[row + x0..row + x1]);
    }
    let thickness = (image.width as f64 / logical_width.max(1) as f64)
        .round()
        .max(1.0) as u32;
    let t = thickness.min(rect.w).min(rect.h);
    let mut fill = |x: u32, y: u32, w: u32, h: u32| {
        for yy in y..y + h {
            let row = yy as usize * stride;
            for px in canvas[row + x as usize * 4..row + (x + w) as usize * 4].chunks_exact_mut(4) {
                px.copy_from_slice(&BORDER);
            }
        }
    };
    fill(rect.x, rect.y, rect.w, t);
    fill(rect.x, rect.y + rect.h - t, rect.w, t);
    fill(rect.x, rect.y, t, rect.h);
    fill(rect.x + rect.w - t, rect.y, t, rect.h);
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if let Some(index) = self.view_index(surface) {
            self.views[index].frame_pending = false;
            self.draw(index, qh);
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.done = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let Some(index) = self.view_index(layer.wl_surface()) else {
            return;
        };
        let view = &mut self.views[index];
        let (w, h) = configure.new_size;
        view.logical = if w == 0 || h == 0 {
            (view.image.width, view.image.height)
        } else {
            (w, h)
        };
        view.configured = true;
        view.dirty = true;
        self.draw(index, qh);
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            let surface = self.compositor.create_surface(qh);
            self.pointer = self
                .seat_state
                .get_pointer_with_theme(qh, &seat, self.shm.wl_shm(), surface, ThemeSpec::default())
                .ok();
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            if let Some(keyboard) = self.keyboard.take() {
                keyboard.release();
            }
        }
        if capability == Capability::Pointer {
            self.pointer = None;
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for App {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        if event.keysym == Keysym::Escape {
            self.done = true;
        }
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
    }
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let index = self.view_index(&event.surface);
            match event.kind {
                PointerEventKind::Enter { .. } => {
                    if let Some(pointer) = &self.pointer {
                        let _ = pointer.set_cursor(conn, CursorIcon::Crosshair);
                    }
                }
                PointerEventKind::Press {
                    button: BTN_LEFT, ..
                } => {
                    if let Some(index) = index {
                        self.drag = Some(Drag {
                            view: index,
                            start: event.position,
                            end: event.position,
                        });
                    }
                }
                PointerEventKind::Motion { .. } => {
                    if let Some(drag) = self.drag.as_mut().filter(|drag| Some(drag.view) == index) {
                        drag.end = event.position;
                        let view = drag.view;
                        self.redraw(view, qh);
                    }
                }
                PointerEventKind::Release {
                    button: BTN_LEFT, ..
                } => {
                    if let Some(drag) = self.drag.as_mut().filter(|drag| Some(drag.view) == index) {
                        drag.end = event.position;
                    }
                    if self.drag.is_some() {
                        self.finish();
                    }
                }
                PointerEventKind::Press {
                    button: BTN_RIGHT, ..
                } => {
                    self.done = true;
                }
                _ => {}
            }
        }
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl AsMut<SimpleGlobal<WpViewporter, 1>> for App {
    fn as_mut(&mut self) -> &mut SimpleGlobal<WpViewporter, 1> {
        &mut self.viewporter
    }
}

impl Dispatch<WpViewport, GlobalData> for App {
    fn event(
        _: &mut Self,
        _: &WpViewport,
        _: <WpViewport as wayland_client::Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_keyboard!(App);
delegate_pointer!(App);
delegate_layer!(App);
delegate_registry!(App);
delegate_simple!(App, WpViewporter, 1);
