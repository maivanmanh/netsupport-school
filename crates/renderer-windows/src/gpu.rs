//! A single reusable BGRA upload texture, GPU aspect scaling and a two-buffer
//! flip swap chain. All COM objects and the immediate context stay on the HWND
//! owner thread. No decoded frames or GPU submissions are queued by this module.
use media_core::DecodedFrameRef;
use std::{ptr, slice, time::Instant};
use windows::{
    core::{s, Interface, Result, PCSTR},
    Win32::{
        Foundation::{HMODULE, HWND},
        Graphics::{
            Direct3D::{
                Fxc::D3DCompile, ID3DBlob, ID3DInclude, D3D_DRIVER_TYPE_HARDWARE,
                D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0,
                D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            },
            Direct3D11::{
                D3D11CreateDeviceAndSwapChain, ID3D11Device, ID3D11DeviceContext,
                ID3D11PixelShader, ID3D11RenderTargetView, ID3D11SamplerState,
                ID3D11ShaderResourceView, ID3D11Texture2D, ID3D11VertexShader,
                D3D11_BIND_SHADER_RESOURCE, D3D11_COMPARISON_ALWAYS,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                D3D11_FILTER_MIN_MAG_MIP_POINT, D3D11_SAMPLER_DESC, D3D11_SDK_VERSION,
                D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_USAGE_DEFAULT,
                D3D11_VIEWPORT,
            },
            Dxgi::{
                Common::{
                    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8_UNORM, DXGI_FORMAT_R8_UNORM,
                    DXGI_MODE_DESC, DXGI_SAMPLE_DESC,
                },
                IDXGIDevice, IDXGIDevice1, IDXGIFactory, IDXGISwapChain, DXGI_MWA_NO_ALT_ENTER,
                DXGI_PRESENT, DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_CHAIN_FLAG,
                DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
            },
        },
    },
};

const SHADER: &[u8] = br"
struct Vertex { float4 position : SV_Position; float2 uv : TEXCOORD0; };
Vertex vertex(uint id : SV_VertexID) {
    Vertex output;
    output.uv = float2((id << 1) & 2, id & 2);
    output.position = float4(output.uv * float2(2, -2) + float2(-1, 1), 0, 1);
    return output;
}
Texture2D image : register(t0);
Texture2D chroma : register(t1);
SamplerState linear_sampler : register(s0);
SamplerState chroma_sampler : register(s1);
float4 pixel(Vertex input) : SV_Target {
    return float4(image.Sample(linear_sampler, input.uv).rgb, 1);
}
float4 pixel_nv12(Vertex input) : SV_Target {
    float c = max(image.Sample(linear_sampler, input.uv).r * 255 - 16, 0);
    float2 de = chroma.Sample(chroma_sampler, input.uv).rg * 255 - 128;
    float3 rgb = float3(298*c + 459*de.y, 298*c - 55*de.x - 136*de.y, 298*c + 541*de.x);
    return float4(saturate((rgb + 128) / 65280), 1);
}
";

pub struct GpuRenderer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swap_chain: IDXGISwapChain,
    target: Option<ID3D11RenderTargetView>,
    vertex: ID3D11VertexShader,
    pixel: ID3D11PixelShader,
    nv12_pixel: ID3D11PixelShader,
    chroma_texture: Option<ID3D11Texture2D>,
    chroma_image: Option<ID3D11ShaderResourceView>,
    source_format: media_core::DecodedPixelFormat,
    diagnostics: media_core::diagnostics::StageMetrics,
    timing: Option<crate::gpu_timing::GpuTiming>,
    sampler: ID3D11SamplerState,
    chroma_sampler: ID3D11SamplerState,
    texture: Option<ID3D11Texture2D>,
    image: Option<ID3D11ShaderResourceView>,
    source_size: (u32, u32),
    client_size: (u32, u32),
}

impl GpuRenderer {
    pub fn new(window: HWND, width: u32, height: u32) -> Result<Self> {
        // SAFETY: HWND belongs to this thread; output pointers are initialized
        // Options. D3D retains its own resource references, managed by windows-rs.
        unsafe {
            let desc = DXGI_SWAP_CHAIN_DESC {
                BufferDesc: DXGI_MODE_DESC {
                    Width: width,
                    Height: height,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    ..Default::default()
                },
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                OutputWindow: window,
                Windowed: true.into(),
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                ..Default::default()
            };
            let (mut device, mut context, mut swap_chain) = (None, None, None);
            D3D11CreateDeviceAndSwapChain(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_0]),
                D3D11_SDK_VERSION,
                Some(ptr::addr_of!(desc)),
                Some(ptr::addr_of_mut!(swap_chain)),
                Some(ptr::addr_of_mut!(device)),
                None,
                Some(ptr::addr_of_mut!(context)),
            )?;
            let device = device.unwrap();
            let context = context.unwrap();
            let swap_chain = swap_chain.unwrap();
            // Limit driver-side buffering as well as the application slot.
            let dxgi: IDXGIDevice1 = device.cast()?;
            dxgi.SetMaximumFrameLatency(1)?;
            let dxgi_base: IDXGIDevice = device.cast()?;
            let adapter_handle = dxgi_base.GetAdapter()?;
            let factory: IDXGIFactory = adapter_handle.GetParent()?;
            factory.MakeWindowAssociation(window, DXGI_MWA_NO_ALT_ENTER)?;
            let adapter = adapter_handle.GetDesc()?;
            let adapter_name = String::from_utf16_lossy(&adapter.Description)
                .trim_end_matches('\0')
                .to_owned();
            let vertex_code = compile(s!("vertex"), s!("vs_4_0"))?;
            let pixel_code = compile(s!("pixel"), s!("ps_4_0"))?;
            let nv12_code = compile(s!("pixel_nv12"), s!("ps_4_0"))?;
            let mut nv12_pixel = None;
            device.CreatePixelShader(
                blob_bytes(&nv12_code),
                None,
                Some(ptr::addr_of_mut!(nv12_pixel)),
            )?;
            let (mut vertex, mut pixel) = (None, None);
            device.CreateVertexShader(
                blob_bytes(&vertex_code),
                None,
                Some(ptr::addr_of_mut!(vertex)),
            )?;
            device.CreatePixelShader(
                blob_bytes(&pixel_code),
                None,
                Some(ptr::addr_of_mut!(pixel)),
            )?;
            let (linear_sampler, chroma_sampler) = make_samplers(&device)?;
            let timing = crate::gpu_timing::GpuTiming::new(&device).map_err(|error| { tracing::warn!(%error, "GPU timestamp queries unavailable; CPU timings remain enabled"); error }).ok();
            let mut renderer = Self {
                timing,
                device,
                context,
                swap_chain,
                target: None,
                vertex: vertex.unwrap(),
                pixel: pixel.unwrap(),
                nv12_pixel: nv12_pixel.unwrap(),
                chroma_texture: None,
                chroma_image: None,
                source_format: media_core::DecodedPixelFormat::Bgra,
                diagnostics: media_core::diagnostics::StageMetrics::default(),
                sampler: linear_sampler,
                chroma_sampler,
                texture: None,
                image: None,
                source_size: (0, 0),
                client_size: (width, height),
            };
            renderer.create_target()?;
            tracing::info!(
                renderer_backend = "d3d11",
                gpu_adapter = adapter_name,
                driver_frame_latency = 1,
                gpu_color_conversion = "nv12_bt709_limited",
                "Windows renderer backend ready"
            );
            Ok(renderer)
        }
    }

    fn create_target(&mut self) -> Result<()> {
        // SAFETY: swap chain is valid; the temporary back-buffer reference is
        // released after the RTV takes a reference to it.
        unsafe {
            let back: ID3D11Texture2D = self.swap_chain.GetBuffer(0)?;
            self.device
                .CreateRenderTargetView(&back, None, Some(ptr::addr_of_mut!(self.target)))
        }
    }

    pub fn take_diagnostics(&mut self) -> media_core::diagnostics::StageMetrics {
        if let Some(timing) = &mut self.timing {
            timing.collect(&self.context, &mut self.diagnostics);
            timing.report();
        }
        std::mem::take(&mut self.diagnostics)
    }

    #[allow(clippy::cast_precision_loss, clippy::too_many_lines)] // Display sizes/coordinates are exact at this scale.
    pub fn render(&mut self, frame: DecodedFrameRef<'_>, width: u32, height: u32) -> Result<()> {
        // SAFETY: caller validates tightly packed width*height*4 BGRA storage.
        // The input slice stays alive until UpdateSubresource completes its copy.
        unsafe {
            let upload_started = Instant::now();
            let nv12 = matches!(frame.format, media_core::DecodedPixelFormat::Nv12 { .. });
            if self.client_size != (width, height) {
                self.context.OMSetRenderTargets(None, None);
                self.target = None;
                self.swap_chain.ResizeBuffers(
                    2,
                    width,
                    height,
                    DXGI_FORMAT_B8G8R8A8_UNORM,
                    DXGI_SWAP_CHAIN_FLAG(0),
                )?;
                self.client_size = (width, height);
                self.create_target()?;
            }
            if self.source_size != (frame.width, frame.height) || self.source_format != frame.format
            {
                self.context.PSSetShaderResources(0, Some(&[None, None]));
                self.chroma_texture = None;
                self.chroma_image = None;
                self.image = None;
                self.texture = None;
                let desc = D3D11_TEXTURE2D_DESC {
                    Width: frame.width,
                    Height: frame.height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: if nv12 {
                        DXGI_FORMAT_R8_UNORM
                    } else {
                        DXGI_FORMAT_B8G8R8A8_UNORM
                    },
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                    ..Default::default()
                };
                self.device.CreateTexture2D(
                    ptr::addr_of!(desc),
                    None,
                    Some(ptr::addr_of_mut!(self.texture)),
                )?;
                self.device.CreateShaderResourceView(
                    self.texture.as_ref().unwrap(),
                    None,
                    Some(ptr::addr_of_mut!(self.image)),
                )?;
                if nv12 {
                    let chroma_desc = D3D11_TEXTURE2D_DESC {
                        Width: frame.width / 2,
                        Height: frame.height / 2,
                        Format: DXGI_FORMAT_R8G8_UNORM,
                        ..desc
                    };
                    self.device.CreateTexture2D(
                        ptr::addr_of!(chroma_desc),
                        None,
                        Some(ptr::addr_of_mut!(self.chroma_texture)),
                    )?;
                    self.device.CreateShaderResourceView(
                        self.chroma_texture.as_ref().unwrap(),
                        None,
                        Some(ptr::addr_of_mut!(self.chroma_image)),
                    )?;
                }
                self.source_size = (frame.width, frame.height);
                self.source_format = frame.format;
            }
            if let Some(timing) = &mut self.timing {
                timing.begin(&self.context, &mut self.diagnostics);
            }
            let row_pitch = match frame.format {
                media_core::DecodedPixelFormat::Bgra => frame.width * 4,
                media_core::DecodedPixelFormat::Nv12 {
                    stride,
                    storage_height,
                } => {
                    // Validated buffer: UV starts after padded luma storage, not visible height.
                    let uv = &frame.bytes[usize::try_from(
                        u64::from(stride) * u64::from(storage_height),
                    )
                    .map_err(|_| {
                        windows::core::Error::from_hresult(windows::Win32::Foundation::E_INVALIDARG)
                    })?..];
                    self.context.UpdateSubresource(
                        self.chroma_texture.as_ref().unwrap(),
                        0,
                        None,
                        uv.as_ptr().cast(),
                        stride,
                        0,
                    );
                    stride
                }
            };
            self.context.UpdateSubresource(
                self.texture.as_ref().unwrap(),
                0,
                None,
                frame.bytes.as_ptr().cast(),
                row_pitch,
                0,
            );
            self.diagnostics
                .record("render_upload_cpu", upload_started.elapsed());
            let draw_started = Instant::now();
            let (fit_width, fit_height) = crate::fit::fit_rect(
                frame.width,
                frame.height,
                i32::try_from(width).unwrap_or(i32::MAX),
                i32::try_from(height).unwrap_or(i32::MAX),
            );
            let view = D3D11_VIEWPORT {
                TopLeftX: (width as f32 - fit_width as f32) / 2.0,
                TopLeftY: (height as f32 - fit_height as f32) / 2.0,
                Width: fit_width as f32,
                Height: fit_height as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            };
            self.context
                .ClearRenderTargetView(self.target.as_ref(), &[0.0, 0.0, 0.0, 1.0]);
            self.context
                .OMSetRenderTargets(Some(slice::from_ref(&self.target)), None);
            self.context.RSSetViewports(Some(&[view]));
            self.context
                .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            self.context.VSSetShader(&self.vertex, None);
            self.context
                .PSSetShader(if nv12 { &self.nv12_pixel } else { &self.pixel }, None);
            self.context
                .PSSetShaderResources(0, Some(&[self.image.clone(), self.chroma_image.clone()]));
            self.context.PSSetSamplers(
                0,
                Some(&[
                    Some(self.sampler.clone()),
                    Some(self.chroma_sampler.clone()),
                ]),
            );
            self.context.Draw(3, 0);
            if let Some(timing) = &mut self.timing {
                timing.end(&self.context);
            }
            self.diagnostics
                .record("render_draw_submit_cpu", draw_started.elapsed());
            let present_started = Instant::now();
            let result = self.swap_chain.Present(0, DXGI_PRESENT(0)).ok();
            self.diagnostics
                .record("render_present_cpu", present_started.elapsed());
            result
        }
    }
}

fn make_samplers(device: &ID3D11Device) -> Result<(ID3D11SamplerState, ID3D11SamplerState)> {
    // SAFETY: descriptors are valid values and returned COM references remain owned.
    unsafe {
        let mut sampler = None;
        let sampler_desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            MaxLOD: f32::MAX,
            MaxAnisotropy: 1,
            ComparisonFunc: D3D11_COMPARISON_ALWAYS,
            ..Default::default()
        };
        device.CreateSamplerState(
            ptr::addr_of!(sampler_desc),
            Some(ptr::addr_of_mut!(sampler)),
        )?;
        let mut chroma_sampler = None;
        let chroma_desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_POINT,
            ..sampler_desc
        };
        device.CreateSamplerState(
            ptr::addr_of!(chroma_desc),
            Some(ptr::addr_of_mut!(chroma_sampler)),
        )?;
        Ok((sampler.unwrap(), chroma_sampler.unwrap()))
    }
}

fn compile(entry: PCSTR, profile: PCSTR) -> Result<ID3DBlob> {
    let mut code = None;
    // SAFETY: shader is static valid UTF-8 HLSL; compiler reads exactly its slice
    // length. No includes or filesystem access; returned blob owns its bytes.
    unsafe {
        D3DCompile(
            SHADER.as_ptr().cast(),
            SHADER.len(),
            None,
            None,
            None::<&ID3DInclude>,
            entry,
            profile,
            0,
            0,
            ptr::addr_of_mut!(code),
            None,
        )?;
    }
    Ok(code.unwrap())
}

unsafe fn blob_bytes(blob: &ID3DBlob) -> &[u8] {
    // SAFETY: blob retains the byte storage for the borrow's entire lifetime.
    unsafe { slice::from_raw_parts(blob.GetBufferPointer().cast(), blob.GetBufferSize()) }
}
