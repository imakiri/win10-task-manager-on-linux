/* Copyright 2026 Mission Center Developers
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! EGL on the launcher's `wl_surface` and the egui_glow painter. The context lives for the whole
//! process (textures stay uploaded); only the window surface comes and goes with the launcher.

use std::ffi::c_void;
use std::num::NonZeroU32;
use std::ptr::NonNull;
use std::sync::Arc;

use glutin::api::egl::context::{NotCurrentContext, PossiblyCurrentContext};
use glutin::api::egl::display::Display;
use glutin::api::egl::surface::Surface;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, Version};
use glutin::prelude::*;
use glutin::surface::{SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};

enum Context {
    NotCurrent(NotCurrentContext),
    Current(PossiblyCurrentContext),
    /// Only while switching states
    Gone,
}

pub struct Gl {
    display: Display,
    config: glutin::api::egl::config::Config,
    context: Context,
    surface: Option<Surface<WindowSurface>>,
    painter: Option<egui_glow::Painter>,
}

impl Gl {
    /// # Safety
    /// `display` must be a live `wl_display` that outlives this value.
    pub unsafe fn new(display: NonNull<c_void>) -> Result<Self, String> {
        let raw = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display));
        // SAFETY: guaranteed by the caller
        let display = unsafe { Display::new(raw) }.map_err(|e| e.to_string())?;
        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(0)
            .with_transparency(false)
            .build();
        // SAFETY: the configs belong to this display
        let config = unsafe { display.find_configs(template) }
            .map_err(|e| e.to_string())?
            .next()
            .ok_or("No EGL configuration")?;
        // Desktop OpenGL first, then OpenGL ES (as eframe's glow renderer)
        let context = [
            ContextAttributesBuilder::new().build(None),
            ContextAttributesBuilder::new()
                .with_context_api(ContextApi::Gles(Some(Version::new(3, 0))))
                .build(None),
        ]
        .iter()
        // SAFETY: the config belongs to this display
        .find_map(|attributes| unsafe { display.create_context(&config, attributes) }.ok())
        .ok_or("Could not create an OpenGL context")?;
        Ok(Self {
            display,
            config,
            context: Context::NotCurrent(context),
            surface: None,
            painter: None,
        })
    }

    /// Creates or resizes the window surface of `wl_surface` (physical pixels).
    ///
    /// # Safety
    /// `surface` must be a live `wl_surface` that outlives the window surface ([`Self::detach`]).
    pub unsafe fn attach(
        &mut self,
        surface: NonNull<c_void>,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let (Some(width), Some(height)) = (NonZeroU32::new(width), NonZeroU32::new(height)) else {
            return Ok(());
        };
        if let (Some(egl_surface), Context::Current(context)) = (&self.surface, &self.context) {
            egl_surface.resize(context, width, height);
            return Ok(());
        }
        let raw = RawWindowHandle::Wayland(WaylandWindowHandle::new(surface));
        let attributes = SurfaceAttributesBuilder::<WindowSurface>::new().build(raw, width, height);
        // SAFETY: guaranteed by the caller
        let egl_surface = unsafe {
            self.display
                .create_window_surface(&self.config, &attributes)
        }
        .map_err(|e| e.to_string())?;
        let context = match std::mem::replace(&mut self.context, Context::Gone) {
            Context::NotCurrent(context) => context,
            Context::Current(context) => context.make_not_current().map_err(|e| e.to_string())?,
            Context::Gone => return Err("OpenGL context lost".into()),
        };
        let context = context
            .make_current(&egl_surface)
            .map_err(|e| e.to_string())?;
        // Frames are drawn only on events; never block in eglSwapBuffers on a hidden surface
        let _ = egl_surface.set_swap_interval(&context, SwapInterval::DontWait);
        self.context = Context::Current(context);
        self.surface = Some(egl_surface);
        if self.painter.is_none() {
            let display = &self.display;
            // SAFETY: the context is current on this thread
            let gl = unsafe {
                glow::Context::from_loader_function_cstr(|name| {
                    display.get_proc_address(name).cast()
                })
            };
            self.painter = Some(
                egui_glow::Painter::new(Arc::new(gl), "", None, true).map_err(|e| e.to_string())?,
            );
        }
        Ok(())
    }

    /// Destroys the window surface before its `wl_surface` goes away.
    pub fn detach(&mut self) {
        if let Context::Current(context) = std::mem::replace(&mut self.context, Context::Gone) {
            match context.make_not_current() {
                Ok(context) => self.context = Context::NotCurrent(context),
                Err(error) => log::warn!("Could not release the OpenGL context: {error}"),
            }
        }
        self.surface = None;
    }

    /// Paints one frame; textures are updated even without a surface (nothing to show).
    pub fn paint(
        &mut self,
        size: [u32; 2],
        pixels_per_point: f32,
        primitives: &[egui::ClippedPrimitive],
        textures: &mut egui::TexturesDelta,
    ) {
        let (Some(surface), Context::Current(context), Some(painter)) =
            (&self.surface, &self.context, &mut self.painter)
        else {
            return;
        };
        egui_glow::painter::clear(painter.gl(), size, [0., 0., 0., 1.]);
        painter.paint_and_update_textures(size, pixels_per_point, primitives, textures);
        if let Err(error) = surface.swap_buffers(context) {
            log::warn!("eglSwapBuffers failed: {error}");
        }
    }
}
