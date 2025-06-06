use std::ffi::c_void;
use std::{ptr, time::Instant};

use imgui::{FontConfig, FontSource};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND};
use windows::Win32::Graphics::Direct3D9::{
    Direct3DCreate9, IDirect3D9, IDirect3DDevice9, D3DADAPTER_DEFAULT, D3DCLEAR_TARGET, D3DCREATE_SOFTWARE_VERTEXPROCESSING, D3DDEVTYPE_HAL, D3DFMT_R5G6B5, D3DMULTISAMPLE_NONE, D3DPRESENT_INTERVAL_DEFAULT, D3DPRESENT_PARAMETERS, D3DPRESENT_RATE_DEFAULT, D3DSWAPEFFECT_DISCARD, D3D_SDK_VERSION
};

use winit::window::WindowAttributes;
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::EventLoop,
};

const WINDOW_WIDTH: f64 = 760.0;
const WINDOW_HEIGHT: f64 = 760.0;

unsafe fn set_up_dx_context(hwnd: HWND) -> (IDirect3D9, IDirect3DDevice9) {
    let d9_option = Direct3DCreate9(D3D_SDK_VERSION);
    match d9_option {
        Some(d9) => {
            let mut present_params = D3DPRESENT_PARAMETERS {
                BackBufferCount: 1,
                MultiSampleType: D3DMULTISAMPLE_NONE,
                MultiSampleQuality: 0,
                SwapEffect: D3DSWAPEFFECT_DISCARD,
                hDeviceWindow: hwnd,
                Flags: 0,
                FullScreen_RefreshRateInHz: D3DPRESENT_RATE_DEFAULT,
                PresentationInterval: D3DPRESENT_INTERVAL_DEFAULT as u32,
                BackBufferFormat: D3DFMT_R5G6B5,
                EnableAutoDepthStencil: BOOL(0),
                Windowed: BOOL(1),
                BackBufferWidth: WINDOW_WIDTH as _,
                BackBufferHeight: WINDOW_HEIGHT as _,
                ..core::mem::zeroed()
            };
            let mut device: Option<IDirect3DDevice9> = None;
            match d9.CreateDevice(
                D3DADAPTER_DEFAULT,
                D3DDEVTYPE_HAL,
                hwnd,
                D3DCREATE_SOFTWARE_VERTEXPROCESSING as u32,
                &mut present_params,
                &mut device,
            ) {
                Ok(_) => (d9, device.unwrap()),
                _ => panic!("CreateDevice failed"),
            }
        },
        None => panic!("Direct3DCreate9 failed"),
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    #[allow(deprecated)]
    let window = event_loop.create_window(WindowAttributes::default()
    .with_title("imgui_dx9_renderer winit example")
    .with_resizable(false)
    .with_inner_size(LogicalSize { width: WINDOW_WIDTH, height: WINDOW_HEIGHT })).unwrap();

    let hwnd = if let RawWindowHandle::Win32(handle) = window.window_handle().unwrap().as_raw() {
        HWND(isize::from(handle.hwnd) as *mut c_void)
    } else {
        unreachable!()
    };
    let (_d9, device) = unsafe { set_up_dx_context(hwnd) };
    let mut imgui = imgui::Context::create();
    imgui.set_ini_filename(None);
    let mut platform = WinitPlatform::new(&mut imgui);
    platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Rounded);

    let hidpi_factor = platform.hidpi_factor();
    let font_size = (13.0 * hidpi_factor) as f32;
    imgui.fonts().add_font(&[FontSource::DefaultFontData {
        config: Some(FontConfig { size_pixels: font_size, ..FontConfig::default() }),
    }]);
    imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;

    let mut renderer =
        unsafe { imgui_dx9_renderer::Renderer::new(&mut imgui, device.clone()).unwrap() };

    let mut last_frame = Instant::now();

    #[allow(deprecated)]
    event_loop.run(move |event, control_flow,| match event {
        Event::NewEvents(_) => {
            let now = Instant::now();
            imgui.io_mut().update_delta_time(now - last_frame);
            last_frame = now;
        },
        Event::AboutToWait => {
            let io = imgui.io_mut();
            platform.prepare_frame(io, &window).expect("Failed to start frame");
            window.request_redraw();
        },
        Event::WindowEvent { event: WindowEvent::RedrawRequested, .. } => {
            unsafe {
                device
                    .Clear(0, ptr::null_mut(), D3DCLEAR_TARGET as u32, 0xFFAA_AAAA, 1.0, 0)
                    .unwrap();
                device.BeginScene().unwrap();
            }

            let ui = imgui.new_frame();
            ui.window("Hello world")
                .size([300.0, 100.0], imgui::Condition::FirstUseEver)
                .build(|| {
                    ui.text("Hello world!");
                    ui.text("This...is...imgui-rs!");
                    ui.separator();
                    let mouse_pos = ui.io().mouse_pos;
                    ui.text(&format!("Mouse Position: ({:.1},{:.1})", mouse_pos[0], mouse_pos[1]));
                });
            ui.show_demo_window(&mut true);
            platform.prepare_render(ui, &window);
            renderer.render(imgui.render()).unwrap();
            unsafe {
                device.EndScene().unwrap();
                device.Present(ptr::null_mut(), ptr::null_mut(), HWND(ptr::null_mut()), ptr::null_mut()).unwrap();
            }
        },
        Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
            control_flow.exit();
        },
        event => {
            platform.handle_event(imgui.io_mut(), &window, &event);
        },
    }).unwrap();
}
