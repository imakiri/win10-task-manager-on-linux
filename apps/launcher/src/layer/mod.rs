/* Copyright 2026 Mission Center Developers
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! Fullscreen launcher surface: a wlr-layer-shell surface on the overlay layer, above windows and
//! panels, on the screen the compositor considers active. winit (eframe) cannot create layer
//! surfaces, so this module drives egui itself: Wayland input → `egui::RawInput`, `App::logic` +
//! `App::ui`, egui_glow painting.
//!
//! Hiding destroys the layer surface; the process stays resident with the EGL context, egui state
//! and the magpie client, so the next activation shows current data and graph history at once.

mod gl;
mod input;

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use calloop::EventLoop;
use calloop::ping::Ping;
use calloop_wayland_source::WaylandSource;
use egui::{Event, Pos2, ViewportCommand, ViewportId, pos2, vec2};
use mc_ui::App;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardHandler, Keysym, Modifiers as WlModifiers, RawModifiers,
};
use smithay_client_toolkit::seat::pointer::{
    PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer,
};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface};
use wayland_client::{Connection, Proxy, QueueHandle};

const NAMESPACE: &str = "taskmanager-launcher";

/// Requests from other launches of the launcher.
pub trait Activation {
    /// Another launch asked to show or hide the launcher since the last call.
    fn take(&self) -> bool;
    /// Wakes the event loop (a repaint request) when [`Self::take`] becomes true.
    fn attach(&mut self, ctx: egui::Context);
}

/// Runs the launcher until it quits (Ctrl+Q or the compositor goes away).
pub fn run(
    make_app: impl FnOnce(&egui::Context) -> App,
    mut activation: Option<Box<dyn Activation>>,
    max_texture_side: usize,
    start_hidden: bool,
) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
    let (globals, queue) = registry_queue_init::<State>(&conn).map_err(|e| e.to_string())?;
    let qh = queue.handle();
    let layer_shell = LayerShell::bind(&globals, &qh).map_err(|_| {
        "The fullscreen launcher needs a compositor with wlr-layer-shell (KDE Plasma, sway, Hyprland…)"
            .to_owned()
    })?;
    let compositor = CompositorState::bind(&globals, &qh).map_err(|e| e.to_string())?;
    let shm = Shm::bind(&globals, &qh).map_err(|e| e.to_string())?;

    let display =
        NonNull::new(conn.backend().display_ptr().cast::<c_void>()).ok_or("No Wayland display")?;
    // SAFETY: the connection (and its display) lives until `run` returns, after `State`
    let gl = unsafe { gl::Gl::new(display) }?;
    // SAFETY: as above
    let clipboard = unsafe { smithay_clipboard::Clipboard::new(display.as_ptr()) };

    let mut event_loop: EventLoop<State> = EventLoop::try_new().map_err(|e| e.to_string())?;
    WaylandSource::new(conn.clone(), queue)
        .insert(event_loop.handle())
        .map_err(|e| e.to_string())?;

    // egui asks for repaints from any thread (magpie data, icons, D-Bus activation)
    let (ping, ping_source) = calloop::ping::make_ping().map_err(|e| e.to_string())?;
    event_loop
        .handle()
        .insert_source(ping_source, |_, _, _| {})
        .map_err(|e| e.to_string())?;
    let requested: Arc<Mutex<Option<Instant>>> = Arc::default();
    let ctx = egui::Context::default();
    {
        let requested = requested.clone();
        let ping: Ping = ping;
        ctx.set_request_repaint_callback(move |info| {
            let at = Instant::now() + info.delay;
            let mut next = requested.lock().unwrap_or_else(|e| e.into_inner());
            if next.is_none_or(|next| at < next) {
                *next = Some(at);
                ping.ping();
            }
        });
    }
    if let Some(activation) = &mut activation {
        activation.attach(ctx.clone());
    }
    let app = make_app(&ctx);

    let mut state = State {
        conn: conn.clone(),
        qh: qh.clone(),
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        compositor,
        layer_shell,
        shm,
        layer: None,
        configured: false,
        size: (0, 0),
        scale: 1,
        keyboard: None,
        pointer: None,
        cursor: None,
        gl,
        clipboard,
        ctx,
        app,
        activation,
        start: Instant::now(),
        max_texture_side,
        events: Vec::new(),
        modifiers: egui::Modifiers::NONE,
        pointer_pos: None,
        focused: false,
        textures: egui::TexturesDelta::default(),
        next_frame: Some(Instant::now()),
        requested,
        exit: false,
    };
    if !start_hidden {
        state.show();
    }

    while !state.exit {
        let timeout = state
            .next_frame
            .map(|at| at.saturating_duration_since(Instant::now()));
        event_loop
            .dispatch(timeout, &mut state)
            .map_err(|e| e.to_string())?;
        let requested = state
            .requested
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        state.schedule(requested);
        if state.next_frame.is_some_and(|at| at <= Instant::now()) {
            state.next_frame = None;
            state.frame();
        }
    }
    state.hide();
    Ok(())
}

struct State {
    conn: Connection,
    qh: QueueHandle<State>,
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,

    layer: Option<LayerSurface>,
    configured: bool,
    /// Logical size from the last configure
    size: (u32, u32),
    scale: i32,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<ThemedPointer>,
    cursor: Option<egui::CursorIcon>,
    gl: gl::Gl,
    clipboard: smithay_clipboard::Clipboard,

    ctx: egui::Context,
    app: App,
    activation: Option<Box<dyn Activation>>,
    start: Instant,
    max_texture_side: usize,
    events: Vec<Event>,
    modifiers: egui::Modifiers,
    pointer_pos: Option<Pos2>,
    focused: bool,
    /// Texture changes not yet uploaded (frames while hidden)
    textures: egui::TexturesDelta,
    next_frame: Option<Instant>,
    requested: Arc<Mutex<Option<Instant>>>,
    exit: bool,
}

impl State {
    fn visible(&self) -> bool {
        self.layer.is_some() && self.configured
    }

    fn schedule(&mut self, at: Option<Instant>) {
        if let Some(at) = at
            && self.next_frame.is_none_or(|next| at < next)
        {
            self.next_frame = Some(at);
        }
    }

    fn repaint_now(&mut self) {
        self.schedule(Some(Instant::now()));
    }

    fn show(&mut self) {
        if self.layer.is_some() {
            return;
        }
        let surface = self.compositor.create_surface(&self.qh);
        // No output: the compositor places it on the active screen
        let layer = self.layer_shell.create_layer_surface(
            &self.qh,
            surface,
            Layer::Overlay,
            Some(NAMESPACE),
            None,
        );
        layer.set_anchor(Anchor::all());
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer.commit();
        self.layer = Some(layer);
        self.configured = false;
        self.scale = 1;
    }

    fn hide(&mut self) {
        self.gl.detach();
        self.layer = None;
        self.configured = false;
        self.focused = false;
        self.pointer_pos = None;
        self.cursor = None;
        self.events.clear();
        self.modifiers = egui::Modifiers::NONE;
        self.repaint_now();
    }

    fn surface(&self) -> Option<&wl_surface::WlSurface> {
        self.layer.as_ref().map(LayerSurface::wl_surface)
    }

    fn resize_gl(&mut self) {
        let Some(surface) = self.surface() else {
            return;
        };
        let Some(ptr) = NonNull::new(surface.id().as_ptr().cast::<c_void>()) else {
            return;
        };
        let scale = self.scale.max(1) as u32;
        // SAFETY: the EGL surface is detached in `hide` before the layer surface is dropped
        if let Err(error) = unsafe {
            self.gl
                .attach(ptr, self.size.0 * scale, self.size.1 * scale)
        } {
            log::error!("Could not create the launcher surface: {error}");
            self.exit = true;
        }
    }

    fn raw_input(&mut self) -> egui::RawInput {
        let visible = self.visible();
        let mut raw = egui::RawInput {
            screen_rect: visible.then(|| {
                egui::Rect::from_min_size(Pos2::ZERO, vec2(self.size.0 as f32, self.size.1 as f32))
            }),
            max_texture_side: Some(self.max_texture_side),
            time: Some(self.start.elapsed().as_secs_f64()),
            events: std::mem::take(&mut self.events),
            focused: visible && self.focused,
            ..Default::default()
        };
        let viewport = raw.viewports.entry(ViewportId::ROOT).or_default();
        viewport.native_pixels_per_point = Some(self.scale.max(1) as f32);
        viewport.focused = Some(visible && self.focused);
        // A hidden launcher reads as minimized: magpie lowers its activity
        viewport.minimized = Some(!visible);
        viewport.fullscreen = Some(true);
        viewport.maximized = Some(true);
        raw
    }

    fn frame(&mut self) {
        if self.activation.as_ref().is_some_and(|a| a.take()) {
            if self.layer.is_some() {
                self.hide();
            } else {
                self.show();
            }
        }
        let visible = self.visible();
        let raw = self.raw_input();
        let ctx = self.ctx.clone();
        let app = &mut self.app;
        let output = ctx.run_ui(raw, |ui| {
            app.logic(ui.ctx());
            // Hidden: logic only, as eframe does for hidden windows
            if visible {
                app.ui(ui);
            }
        });

        for command in &output.platform_output.commands {
            match command {
                egui::OutputCommand::CopyText(text) => self.clipboard.store(text.clone()),
                egui::OutputCommand::OpenUrl(url) => open_url(&url.url),
                egui::OutputCommand::CopyImage(_) => {}
            }
        }
        if visible {
            self.set_cursor(output.platform_output.cursor_icon);
        }
        if let Some(viewport) = output.viewport_output.get(&ViewportId::ROOT) {
            if viewport
                .commands
                .iter()
                .any(|command| matches!(command, ViewportCommand::Close))
            {
                self.exit = true;
            }
            if viewport.repaint_delay != Duration::MAX {
                self.schedule(Some(Instant::now() + viewport.repaint_delay));
            }
        }

        self.textures.append(output.textures_delta);
        if visible {
            let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
            let scale = self.scale.max(1) as u32;
            self.gl.paint(
                [self.size.0 * scale, self.size.1 * scale],
                output.pixels_per_point,
                &primitives,
                &mut self.textures,
            );
            self.textures.clear();
            if let Some(layer) = &self.layer {
                layer.commit();
            }
        }
        if self.app.take_hide_request() {
            self.hide();
        }
    }

    fn set_cursor(&mut self, icon: egui::CursorIcon) {
        if self.cursor == Some(icon) || self.pointer_pos.is_none() {
            return;
        }
        self.cursor = Some(icon);
        let Some(pointer) = &self.pointer else {
            return;
        };
        let result = match input::cursor(icon) {
            Some(cursor) => pointer.set_cursor(&self.conn, cursor),
            None => pointer.hide_cursor(),
        };
        if let Err(error) = result {
            log::debug!("Could not set the cursor: {error}");
        }
    }

    fn key(&mut self, event: &KeyEvent, pressed: bool, repeat: bool) {
        let modifiers = self.modifiers;
        if let Some(key) = input::key(event.keysym) {
            if pressed && modifiers.command {
                match key {
                    egui::Key::C => self.events.push(Event::Copy),
                    egui::Key::X => self.events.push(Event::Cut),
                    egui::Key::V => {
                        if let Ok(text) = self.clipboard.load() {
                            self.events.push(Event::Paste(text));
                        }
                    }
                    _ => {}
                }
            }
            self.events.push(Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat,
                modifiers,
            });
        }
        if pressed && let Some(text) = input::text(event.utf8.as_deref(), modifiers) {
            self.events.push(Event::Text(text.to_owned()));
        }
        self.repaint_now();
    }
}

fn open_url(url: &str) {
    if let Err(error) = std::process::Command::new("xdg-open").arg(url).spawn() {
        log::warn!("Could not open {url}: {error}");
    }
}

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        if self.surface() != Some(surface) || factor == self.scale {
            return;
        }
        self.scale = factor;
        surface.set_buffer_scale(factor);
        if self.configured {
            self.resize_gl();
            self.repaint_now();
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}

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

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        // E.g. its screen went away
        self.hide();
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if self.layer.as_ref() != Some(layer) {
            return;
        }
        self.size = (configure.new_size.0.max(1), configure.new_size.1.max(1));
        self.configured = true;
        self.resize_gl();
        self.repaint_now();
    }
}

impl SeatHandler for State {
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
            match self.seat_state.get_keyboard(qh, &seat, None) {
                Ok(keyboard) => self.keyboard = Some(keyboard),
                Err(error) => log::warn!("No keyboard: {error}"),
            }
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            let surface = self.compositor.create_surface(qh);
            match self.seat_state.get_pointer_with_theme(
                qh,
                &seat,
                self.shm.wl_shm(),
                surface,
                ThemeSpec::default(),
            ) {
                Ok(pointer) => self.pointer = Some(pointer),
                Err(error) => log::warn!("No pointer: {error}"),
            }
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard
            && let Some(keyboard) = self.keyboard.take()
        {
            keyboard.release();
        }
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            pointer.pointer().release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        if self.surface() == Some(surface) {
            self.focused = true;
            self.repaint_now();
        }
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if self.surface() == Some(surface) {
            self.focused = false;
            self.modifiers = egui::Modifiers::NONE;
            self.repaint_now();
        }
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(&event, true, false);
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(&event, true, true);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(&event, false, false);
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: WlModifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.modifiers = input::modifiers(modifiers);
        self.events.push(Event::ModifiersChanged(self.modifiers));
        self.repaint_now();
    }
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if self.surface() != Some(&event.surface) {
                continue;
            }
            let pos = pos2(event.position.0 as f32, event.position.1 as f32);
            match event.kind {
                PointerEventKind::Enter { .. } => {
                    self.pointer_pos = Some(pos);
                    // The compositor resets the cursor on enter
                    self.cursor = None;
                    self.events.push(Event::PointerMoved(pos));
                }
                PointerEventKind::Leave { .. } => {
                    self.pointer_pos = None;
                    self.events.push(Event::PointerGone);
                }
                PointerEventKind::Motion { .. } => {
                    self.pointer_pos = Some(pos);
                    self.events.push(Event::PointerMoved(pos));
                }
                PointerEventKind::Press { button, .. }
                | PointerEventKind::Release { button, .. } => {
                    if let Some(button) = input::button(button) {
                        self.events.push(Event::PointerButton {
                            pos,
                            button,
                            pressed: matches!(event.kind, PointerEventKind::Press { .. }),
                            modifiers: self.modifiers,
                        });
                    }
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    ..
                } => {
                    let (unit, delta) = input::scroll(horizontal, vertical);
                    if delta != egui::Vec2::ZERO {
                        self.events.push(Event::MouseWheel {
                            unit,
                            delta,
                            phase: egui::TouchPhase::Move,
                            modifiers: self.modifiers,
                        });
                    }
                }
            }
        }
        self.repaint_now();
    }
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(State);
delegate_output!(State);
delegate_shm!(State);
delegate_seat!(State);
delegate_keyboard!(State);
delegate_pointer!(State);
delegate_layer!(State);
delegate_registry!(State);
