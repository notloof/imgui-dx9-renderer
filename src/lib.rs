#![cfg(windows)]
#![deny(missing_docs)]
//! This crate offers a DirectX 9 renderer for the [imgui-rs](https://docs.rs/imgui/*/imgui/) rust bindings.

use std::{mem, ptr, slice};

use imgui::{
    internal::RawWrapper, BackendFlags, Context, DrawCmd, DrawCmdParams, DrawData, DrawIdx,
    TextureId, Textures,
};

use windows::Win32::Graphics::Direct3D9::{
    IDirect3DBaseTexture9, IDirect3DDevice9, IDirect3DIndexBuffer9, IDirect3DStateBlock9, IDirect3DTexture9, IDirect3DVertexBuffer9, D3DBLENDOP_ADD, D3DBLEND_INVSRCALPHA, D3DBLEND_ONE, D3DBLEND_SRCALPHA, D3DCULL_NONE, D3DFILL_SOLID, D3DFMT_A8R8G8B8, D3DFMT_INDEX16, D3DFMT_INDEX32, D3DFVF_DIFFUSE, D3DFVF_TEX1, D3DFVF_XYZ, D3DLOCKED_RECT, D3DLOCK_DISCARD, D3DPOOL_DEFAULT, D3DPT_TRIANGLELIST, D3DRS_ALPHABLENDENABLE, D3DRS_ALPHATESTENABLE, D3DRS_BLENDOP, D3DRS_CLIPPING, D3DRS_CULLMODE, D3DRS_DESTBLEND, D3DRS_DESTBLENDALPHA, D3DRS_FILLMODE, D3DRS_FOGENABLE, D3DRS_LIGHTING, D3DRS_RANGEFOGENABLE, D3DRS_SCISSORTESTENABLE, D3DRS_SEPARATEALPHABLENDENABLE, D3DRS_SHADEMODE, D3DRS_SPECULARENABLE, D3DRS_SRCBLEND, D3DRS_SRCBLENDALPHA, D3DRS_STENCILENABLE, D3DRS_ZENABLE, D3DRS_ZWRITEENABLE, D3DSAMP_MAGFILTER, D3DSAMP_MINFILTER, D3DSBT_ALL, D3DSHADE_GOURAUD, D3DTA_DIFFUSE, D3DTA_TEXTURE, D3DTEXF_LINEAR, D3DTOP_DISABLE, D3DTOP_MODULATE, D3DTRANSFORMSTATETYPE, D3DTSS_ALPHAARG1, D3DTSS_ALPHAARG2, D3DTSS_ALPHAOP, D3DTSS_COLORARG1, D3DTSS_COLORARG2, D3DTSS_COLOROP, D3DTS_PROJECTION, D3DTS_VIEW, D3DUSAGE_DYNAMIC, D3DUSAGE_WRITEONLY, D3DVIEWPORT9
};

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Dxgi::DXGI_ERROR_INVALID_CALL;

use windows::core::Interface;
use windows_numerics::Matrix4x4;

const FONT_TEX_ID: usize = !0;
const D3DFVF_CUSTOMVERTEX: u32 = D3DFVF_XYZ | D3DFVF_DIFFUSE | D3DFVF_TEX1;

const FALSE: u32 = 0;
const TRUE: u32 = 1;

const VERTEX_BUF_ADD_CAPACITY: usize = 5000;
const INDEX_BUF_ADD_CAPACITY: usize = 10000;

///Reexport of windows::core::Result<T>
pub type Result<T> = windows::core::Result<T>;

const MAT_IDENTITY: Matrix4x4 = Matrix4x4 {
    M11: 1.0,
    M12: 0.0,
    M13: 0.0,
    M14: 0.0,
    M21: 0.0,
    M22: 1.0,
    M23: 0.0,
    M24: 0.0,
    M31: 0.0,
    M32: 0.0,
    M33: 1.0,
    M34: 0.0,
    M41: 0.0,
    M42: 0.0,
    M43: 0.0,
    M44: 1.0,
};

#[repr(C)]
struct CustomVertex {
    pos: [f32; 3],
    col: [u8; 4],
    uv: [f32; 2],
}

/// A DirectX 9 renderer for (Imgui-rs)[https://docs.rs/imgui/*/imgui/].
pub struct Renderer {
    device: IDirect3DDevice9,
    font_tex: IDirect3DBaseTexture9,
    vertex_buffer: (IDirect3DVertexBuffer9, usize),
    index_buffer: (IDirect3DIndexBuffer9, usize),
    textures: Textures<IDirect3DBaseTexture9>,
}

impl Renderer {
    /// Creates a new renderer for the given [`IDirect3DDevice9`].
    ///
    /// # Safety
    ///
    /// `device` must be a valid [`IDirect3DDevice9`] pointer.
    ///
    /// [`IDirect3DDevice9`]: https://docs.rs/winapi/0.3/x86_64-pc-windows-msvc/winapi/shared/d3d9/struct.IDirect3DDevice9.html
    pub unsafe fn new(ctx: &mut Context, device: IDirect3DDevice9) -> Result<Self> {
		let t = Self::create_font_texture(ctx.fonts(), &device)?;
        let font_tex: IDirect3DBaseTexture9 = t.cast()?;

        ctx.io_mut().backend_flags |= BackendFlags::RENDERER_HAS_VTX_OFFSET;
        ctx.set_renderer_name(String::from(concat!(
            "imgui_dx9_renderer@",
            env!("CARGO_PKG_VERSION")
        )));
        Ok(Renderer {
            vertex_buffer: Self::create_vertex_buffer(&device, 0)?,
            index_buffer: Self::create_index_buffer(&device, 0)?,
            device,
            font_tex,
            textures: Textures::new(),
        })
    }

    /// Creates a new renderer for the given [`IDirect3DDevice9`].
    ///
    /// # Safety
    ///
    /// `device` must be a valid [`IDirect3DDevice9`] pointer.
    ///
    /// [`IDirect3DDevice9`]: https://docs.rs/winapi/0.3/x86_64-pc-windows-msvc/winapi/shared/d3d9/struct.IDirect3DDevice9.html
    pub unsafe fn new_raw(im_ctx: &mut imgui::Context, device: IDirect3DDevice9) -> Result<Self> {
        Self::new(im_ctx, device)
    }

    /// The textures registry of this renderer.
    ///
    /// The texture slot at !0 is reserved for the font texture, therefore the
    /// renderer will ignore any texture inserted into said slot.
    #[inline]
    pub fn textures_mut(&mut self) -> &mut Textures<IDirect3DBaseTexture9> {
        &mut self.textures
    }

    /// The textures registry of this renderer.
    #[inline]
    pub fn textures(&self) -> &Textures<IDirect3DBaseTexture9> {
        &self.textures
    }

    /// Renders the given [`Ui`] with this renderer.
    ///
    /// Should the [`DrawData`] contain an invalid texture index the renderer
    /// will return `DXGI_ERROR_INVALID_CALL` and immediately stop rendering.
    ///
    /// [`Ui`]: https://docs.rs/imgui/*/imgui/struct.Ui.html
    pub fn render(&mut self, draw_data: &DrawData) -> Result<()> {
        if draw_data.display_size[0] < 0.0 || draw_data.display_size[1] < 0.0 {
            return Ok(());
        }
        unsafe {
            if self.vertex_buffer.1 < draw_data.total_vtx_count as usize {
                self.vertex_buffer =
                    Self::create_vertex_buffer(&self.device, draw_data.total_vtx_count as usize)?;
            }
            if self.index_buffer.1 < draw_data.total_idx_count as usize {
                self.index_buffer =
                    Self::create_index_buffer(&self.device, draw_data.total_idx_count as usize)?;
            }

            let _state_guard = StateBackup::backup(&self.device)?;

            self.set_render_state(draw_data)?;
            self.write_buffers(draw_data)?;
            self.render_impl(draw_data)
        }
    }

    unsafe fn render_impl(&mut self, draw_data: &DrawData) -> Result<()> {
        let clip_off = draw_data.display_pos;
        let clip_scale = draw_data.framebuffer_scale;
        let mut vertex_offset = 0;
        let mut index_offset = 0;
        let mut last_tex = TextureId::from(FONT_TEX_ID);
        self.device.SetTexture(0, &self.font_tex).unwrap();
        for draw_list in draw_data.draw_lists() {
            for cmd in draw_list.commands() {
                match cmd {
                    DrawCmd::Elements {
                        count,
                        cmd_params: DrawCmdParams { clip_rect, texture_id, .. },
                    } => {
                        if texture_id != last_tex {
                            let texture = if texture_id.id() == FONT_TEX_ID {
                                &self.font_tex
                            } else {
                                self.textures.get(texture_id).ok_or(DXGI_ERROR_INVALID_CALL)?
                            };
                            self.device.SetTexture(0, texture)?;
                            last_tex = texture_id;
                        }

                        let r: RECT = RECT {
                            left: ((clip_rect[0] - clip_off[0]) * clip_scale[0]) as i32,
                            top: ((clip_rect[1] - clip_off[1]) * clip_scale[1]) as i32,
                            right: ((clip_rect[2] - clip_off[0]) * clip_scale[0]) as i32,
                            bottom: ((clip_rect[3] - clip_off[1]) * clip_scale[1]) as i32,
                        };
                        self.device.SetScissorRect(&r)?;
                        self.device.DrawIndexedPrimitive(
                            D3DPT_TRIANGLELIST,
                            vertex_offset as i32,
                            0,
                            draw_list.vtx_buffer().len() as u32,
                            index_offset as u32,
                            count as u32 / 3,
                        )?;
                        index_offset += count;
                    },
                    DrawCmd::ResetRenderState => self.set_render_state(draw_data)?,
                    DrawCmd::RawCallback { callback, raw_cmd } => {
                        callback(draw_list.raw(), raw_cmd)
                    },
                }
            }
            vertex_offset += draw_list.vtx_buffer().len();
        }
        Ok(())
    }

    unsafe fn set_render_state(&mut self, draw_data: &DrawData) -> Result<()> {
        let fb_width = draw_data.display_size[0] * draw_data.framebuffer_scale[0];
        let fb_height = draw_data.display_size[1] * draw_data.framebuffer_scale[1];

        let vp = D3DVIEWPORT9 {
            X: 0,
            Y: 0,
            Width: fb_width as _,
            Height: fb_height as _,
            MinZ: 0.0,
            MaxZ: 1.0,
        };

        let device = &self.device;
        device.SetViewport(&vp)?;
        device.SetPixelShader(None)?;
        device.SetVertexShader(None)?;
        device.SetRenderState(D3DRS_FILLMODE, D3DFILL_SOLID.0 as u32)?;
        device.SetRenderState(D3DRS_SHADEMODE, D3DSHADE_GOURAUD.0 as u32)?;
        device.SetRenderState(D3DRS_ZWRITEENABLE, FALSE)?;
        device.SetRenderState(D3DRS_ALPHATESTENABLE, FALSE)?;
        device.SetRenderState(D3DRS_CULLMODE, D3DCULL_NONE.0.try_into().unwrap())?;
        device.SetRenderState(D3DRS_ZENABLE, FALSE)?;
        device.SetRenderState(D3DRS_ALPHABLENDENABLE, TRUE)?;
        device.SetRenderState(D3DRS_BLENDOP, D3DBLENDOP_ADD.0.try_into().unwrap())?;
        device.SetRenderState(D3DRS_SRCBLEND, D3DBLEND_SRCALPHA.0.try_into().unwrap())?;
        device.SetRenderState(D3DRS_DESTBLEND, D3DBLEND_INVSRCALPHA.0.try_into().unwrap())?;
        device.SetRenderState(D3DRS_SEPARATEALPHABLENDENABLE, TRUE)?;
        device.SetRenderState(D3DRS_SRCBLENDALPHA, D3DBLEND_ONE.0.try_into().unwrap())?;
        device.SetRenderState(D3DRS_DESTBLENDALPHA, D3DBLEND_INVSRCALPHA.0.try_into().unwrap())?;
        device.SetRenderState(D3DRS_SCISSORTESTENABLE, TRUE)?;
        device.SetRenderState(D3DRS_FOGENABLE, FALSE)?;
        device.SetRenderState(D3DRS_RANGEFOGENABLE, FALSE)?;
        device.SetRenderState(D3DRS_SPECULARENABLE, FALSE)?;
        device.SetRenderState(D3DRS_STENCILENABLE, FALSE)?;
        device.SetRenderState(D3DRS_CLIPPING, TRUE)?;
        device.SetRenderState(D3DRS_LIGHTING, FALSE)?;
        device.SetTextureStageState(0, D3DTSS_COLOROP, D3DTOP_MODULATE.0 as u32)?;
        device.SetTextureStageState(0, D3DTSS_COLORARG1, D3DTA_TEXTURE)?;
        device.SetTextureStageState(0, D3DTSS_COLORARG2, D3DTA_DIFFUSE)?;
        device.SetTextureStageState(0, D3DTSS_ALPHAOP, D3DTOP_MODULATE.0 as u32)?;
        device.SetTextureStageState(0, D3DTSS_ALPHAARG1, D3DTA_TEXTURE)?;
        device.SetTextureStageState(0, D3DTSS_ALPHAARG2, D3DTA_DIFFUSE)?;
        device.SetTextureStageState(1, D3DTSS_COLOROP, D3DTOP_DISABLE.0 as u32)?;
        device.SetTextureStageState(1, D3DTSS_ALPHAOP, D3DTOP_DISABLE.0 as u32)?;
        device.SetSamplerState(0, D3DSAMP_MINFILTER, D3DTEXF_LINEAR.0 as u32)?;
        device.SetSamplerState(0, D3DSAMP_MAGFILTER, D3DTEXF_LINEAR.0 as u32)?;

        let l = draw_data.display_pos[0] + 0.5;
        let r = draw_data.display_pos[0] + draw_data.display_size[0] + 0.5;
        let t = draw_data.display_pos[1] + 0.5;
        let b = draw_data.display_pos[1] + draw_data.display_size[1] + 0.5;
        let mat_projection = Matrix4x4 {
            M11: 2.0 / (r - l),
            M12: 0.0,
            M13: 0.0,
            M14: 0.0,
            M21: 0.0,
            M22: 2.0 / (t - b),
            M23: 0.0,
            M24: 0.0,
            M31: 0.0,
            M32: 0.0,
            M33: 0.5,
            M34: 0.0,
            M41: (l + r) / (l - r),
            M42: (t + b) / (b - t),
            M43: 0.5,
            M44: 1.0,
        };

        device.SetTransform(D3DTRANSFORMSTATETYPE(0), &MAT_IDENTITY)?;
        device.SetTransform(D3DTS_VIEW, &MAT_IDENTITY)?;
        device.SetTransform(D3DTS_PROJECTION, &mat_projection)?;
        Ok(())
    }

    unsafe fn lock_buffers<'v, 'i>(
        vb: &'v mut IDirect3DVertexBuffer9,
        ib: &'i mut IDirect3DIndexBuffer9,
        vtx_count: usize,
        idx_count: usize,
    ) -> Result<(&'v mut [CustomVertex], &'i mut [DrawIdx])> {
        let mut vtx_dst: *mut CustomVertex = ptr::null_mut();
        let mut idx_dst: *mut DrawIdx = ptr::null_mut();

        vb.Lock(
            0,
            (vtx_count * mem::size_of::<CustomVertex>()) as u32,
            &mut vtx_dst as *mut _ as _,
            D3DLOCK_DISCARD as u32,
        )?;

        match ib.Lock(
            0,
            (idx_count * mem::size_of::<DrawIdx>()) as u32,
            &mut idx_dst as *mut _ as _,
            D3DLOCK_DISCARD as u32,
        ) {
            Ok(_) => Ok((
                slice::from_raw_parts_mut(vtx_dst, vtx_count),
                slice::from_raw_parts_mut(idx_dst, idx_count),
            )),
            Err(e) => {
                vb.Unlock()?;
                Err(e)
            },
        }
    }

    unsafe fn write_buffers(&mut self, draw_data: &DrawData) -> Result<()> {
        let (mut vtx_dst, mut idx_dst) = Self::lock_buffers(
            &mut self.vertex_buffer.0,
            &mut self.index_buffer.0,
            draw_data.total_vtx_count as usize,
            draw_data.total_idx_count as usize,
        )?;

        for (vbuf, ibuf) in
            draw_data.draw_lists().map(|draw_list| (draw_list.vtx_buffer(), draw_list.idx_buffer()))
        {
            for (vertex, vtx_dst) in vbuf.iter().zip(vtx_dst.iter_mut()) {
                *vtx_dst = CustomVertex {
                    pos: [vertex.pos[0], vertex.pos[1], 0.0],
                    col: [vertex.col[2], vertex.col[1], vertex.col[0], vertex.col[3]],
                    uv: [vertex.uv[0], vertex.uv[1]],
                };
            }
            idx_dst[..ibuf.len()].copy_from_slice(ibuf);
            vtx_dst = &mut vtx_dst[vbuf.len()..];
            idx_dst = &mut idx_dst[ibuf.len()..];
        }
        self.vertex_buffer.0.Unlock()?;
        self.index_buffer.0.Unlock()?;
        self.device.SetStreamSource(
            0,
            &self.vertex_buffer.0,
            0,
            mem::size_of::<CustomVertex>() as u32,
        )?;
        self.device.SetIndices(&self.index_buffer.0)?;
        self.device.SetFVF(D3DFVF_CUSTOMVERTEX)?;
        Ok(())
    }

    unsafe fn create_vertex_buffer(
        device: &IDirect3DDevice9,
        vtx_count: usize,
    ) -> Result<(IDirect3DVertexBuffer9, usize)> {
        let len = vtx_count + VERTEX_BUF_ADD_CAPACITY;
        let mut vertex_buffer: Option<IDirect3DVertexBuffer9> = None;
        device.CreateVertexBuffer(
            (len * mem::size_of::<CustomVertex>()) as u32,
            (D3DUSAGE_DYNAMIC | D3DUSAGE_WRITEONLY) as u32,
            D3DFVF_CUSTOMVERTEX,
            D3DPOOL_DEFAULT,
            &mut vertex_buffer,
            ptr::null_mut(),
        )?;
        Ok((vertex_buffer.unwrap(), len))
    }

    unsafe fn create_index_buffer(
        device: &IDirect3DDevice9,
        idx_count: usize,
    ) -> Result<(IDirect3DIndexBuffer9, usize)> {
        let len = idx_count + INDEX_BUF_ADD_CAPACITY;
        let mut index_buffer: Option<IDirect3DIndexBuffer9> = None;

        device.CreateIndexBuffer(
            (len * mem::size_of::<DrawIdx>()) as u32,
            (D3DUSAGE_DYNAMIC | D3DUSAGE_WRITEONLY) as u32,
            if mem::size_of::<DrawIdx>() == 2 { D3DFMT_INDEX16 } else { D3DFMT_INDEX32 },
            D3DPOOL_DEFAULT,
            &mut index_buffer,
            ptr::null_mut(),
        )?;
        Ok((index_buffer.unwrap(), len))
    }

    // FIXME, imgui hands us an rgba texture while we make dx9 think it receives an
    // argb texture
    unsafe fn create_font_texture(
        fonts: &mut imgui::FontAtlas,
        device: &IDirect3DDevice9,
    ) -> Result<IDirect3DTexture9> {
        let texture = fonts.build_rgba32_texture();
        let mut texture_handle: Option<IDirect3DTexture9> = None;

        device.CreateTexture(
            texture.width,
            texture.height,
            1,
            D3DUSAGE_DYNAMIC as u32,
            D3DFMT_A8R8G8B8,
            D3DPOOL_DEFAULT,
            &mut texture_handle,
            ptr::null_mut(),
        )?;

        let mut locked_rect: D3DLOCKED_RECT = D3DLOCKED_RECT { Pitch: 0, pBits: ptr::null_mut() };
        let result_texture = texture_handle.unwrap();

        result_texture.LockRect(0, &mut locked_rect, ptr::null_mut(), 0)?;

        let bits = locked_rect.pBits as *mut u8;
        let pitch = locked_rect.Pitch as usize;
        let height = texture.height as usize;
        let width = texture.width as usize;

        for y in 0..height {
            let d3d9_memory = bits.add(pitch * y);
            let pixels = texture.data.as_ptr();
            let pixels = pixels.add((width * 4) * y);
            std::ptr::copy(pixels, d3d9_memory, width * 4);
        }

        result_texture.UnlockRect(0)?;
        fonts.tex_id = TextureId::from(FONT_TEX_ID);
        Ok(result_texture)
    }
}

struct StateBackup(IDirect3DStateBlock9);

impl StateBackup {
    unsafe fn backup(device: &IDirect3DDevice9) -> Result<Self> {
        device.CreateStateBlock(D3DSBT_ALL).map(StateBackup)
    }
}

impl Drop for StateBackup {
    #[inline]
    fn drop(&mut self) {
        unsafe { self.0.Apply().expect("applying state backup failed") };
    }
}
