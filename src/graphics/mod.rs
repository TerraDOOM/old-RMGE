extern crate shaderc;

#[cfg(feature = "dx12")]
use gfx_backend_dx12 as back;
#[cfg(feature = "metal")]
use gfx_backend_metal as back;
#[cfg(feature = "vulkan")]
use gfx_backend_vulkan as back;

macro_rules! debug_repr {
    ($e:expr) => {format_args!("{:?}", $e)};
}

mod gpu_buffer;
mod loadedimage;
mod vertex;

use crate::geometry::Quad;
use arrayvec::ArrayVec;
use core::{
    mem::{self, ManuallyDrop},
    ops::Deref,
};
use gfx_hal::{
    adapter::{Adapter, PhysicalDevice},
    buffer::{IndexBufferView, Usage as BufferUsage},
    command::{ClearColor, ClearValue, CommandBuffer, MultiShot, Primary},
    device::Device,
    format::{Aspects, ChannelType, Format, Swizzle},
    image::{Extent, Layout, Usage, SubresourceRange, ViewKind},
    pass::{Attachment, AttachmentLoadOp, AttachmentOps, AttachmentStoreOp, Subpass, SubpassDesc},
    pool::{CommandPool, CommandPoolCreateFlags},
    pso::{
        AttributeDesc, BakedStates, BasePipeline, BlendDesc, BlendState, ColorBlendDesc,
        ColorMask, DepthStencilDesc, DepthTest, DescriptorSetLayoutBinding, ElemStride,
        EntryPoint, Face,  FrontFace, GraphicsPipelineDesc, GraphicsShaderSet,
        InputAssemblerDesc, LogicOp, PipelineCreationFlags, PipelineStage, PolygonMode, Rasterizer,
        Rect, ShaderStageFlags, Specialization, StencilTest, VertexBufferDesc, Viewport,
    },
    queue::{
        family::QueueGroup,
        Submission,
    },
    window::{Backbuffer, Extent2D, FrameSync, PresentMode, Swapchain, SwapchainConfig},
    Backend, DescriptorPool, Gpu, Graphics, IndexType, Instance, Primitive, QueueFamily, Surface,
};
use gpu_buffer::BufferBundle;
use loadedimage::LoadedImage;
use slog::Logger;
use vertex::Vertex;

const MAX_QUADS: usize = 4096;
const VERTEX_SOURCE: &str = include_str!("vertex.glsl");
const FRAGMENT_SOURCE: &str = include_str!("fragment.glsl");

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct TexturedQuad {
    pub quad: Quad,
    pub uv_rect: [f32; 4],
}

impl TexturedQuad {
    pub(crate) fn to_f32s(self) -> [f32; 4 * (2 + 3 + 2 + 4)] {
        let [uvx, uvy, uvz, uvw] = self.uv_rect;
        let Quad {
            top_left,
            bottom_left,
            bottom_right,
            top_right,
        } = self.quad;
        #[cfg_attr(rustfmt, rustfmt_skip)]
        [/*
         X               Y               R    G    B                  U    V                    */ /* uv_rect       */
         top_left.x,     top_left.y,     1.0, 0.0, 0.0, /* red     */ 0.0, 1.0, /* bottom left  */ uvx, uvy, uvz, uvw,
         bottom_left.x,  bottom_left.y,  0.0, 1.0, 0.0, /* green   */ 0.0, 0.0, /* top left     */ uvx, uvy, uvz, uvw,
         bottom_right.x, bottom_right.y, 0.0, 0.0, 1.0, /* blue    */ 1.0, 0.0, /* bottom right */ uvx, uvy, uvz, uvw,
         top_right.x,    top_right.y,    1.0, 0.0, 1.0, /* magenta */ 1.0, 1.0, /* top right    */ uvx, uvy, uvz, uvw,
        ]
    }
    pub fn to_vertices(self) -> [Vertex; 4] {
        let uv_rect = self.uv_rect;
        let Quad {
            top_left,
            bottom_left,
            bottom_right,
            top_right,
        } = self.quad;
        [
            Vertex {
                xy: [top_left.x, top_left.y],
                rgb: [1.0, 0.0, 0.0],
                uv: [0.0, 1.0],
                uv_rect,
            },
            Vertex {
                xy: [bottom_left.x, bottom_left.y],
                rgb: [0.0, 1.0, 0.0],
                           uv: [0.0, 0.0],
                           uv_rect,
                       },
                       Vertex {
                           xy: [bottom_right.x, bottom_right.y],
                           rgb: [0.0, 0.0, 1.0],
                           uv: [1.0, 0.0],
                           uv_rect,
                       },
                       Vertex {
                           xy: [top_right.x, top_right.y],
                           rgb: [1.0, 0.0, 1.0],
                           uv: [1.0, 1.0],
                           uv_rect,
                       },
                   ]
               }
           }

           #[derive(Copy, Clone, Debug)]
           pub enum Vsync {
               TripleBuffered,
               DoubleBuffered,
               Relaxed,
               Immediate,
           }

           impl Into<PresentMode> for Vsync {
               fn into(self) -> PresentMode {
                   match self {
                       Vsync::TripleBuffered => PresentMode::Mailbox,
                       Vsync::DoubleBuffered => PresentMode::Fifo,
                       Vsync::Relaxed => PresentMode::Relaxed,
                       Vsync::Immediate => PresentMode::Immediate,
                   }
               }
           }

           #[derive(Clone, Debug)]
           pub struct HalStateBuilder<'a, 'b, 'c> {
               window: &'b winit::Window,
               name: Option<&'c str>,
               image: &'a [u8],
               num_quads: Option<usize>,
               preferred_vsync: Option<[PresentMode; 4]>,
               logger: Option<slog::Logger>
           }

           impl<'a, 'b, 'c> HalStateBuilder<'a, 'b, 'c> {
               pub fn new(window: &'b winit::Window, image: &'a [u8]) -> Self {
                   Self {
                       window,
                       image,
                       name: None,
                       num_quads: None,
                       preferred_vsync: None,
                       logger: None,
                   }
               }

               pub fn preferred_vsync(mut self, vsync: [Vsync; 4]) -> Self {
                   self.preferred_vsync = Some([
                       vsync[0].into(),
                       vsync[1].into(),
                       vsync[2].into(),
                       vsync[3].into(),
                   ]);
                   self
               }

               pub fn num_quads(mut self, num_quads: usize) -> Self {
                   self.num_quads = Some(num_quads);
                   self
               }

               pub fn name(mut self, name: &'c str) -> Self {
                   self.name = Some(name);
                   self
               }

               pub fn build(self) -> Result<HalState, &'static str> {
                   use gfx_hal::window::PresentMode::*;
                   let num_quads = self.num_quads.unwrap_or(MAX_QUADS);
                   let vsync = self
                       .preferred_vsync
                       .unwrap_or([Mailbox, Fifo, Relaxed, Immediate]);
                   let name = self.name.unwrap_or("");
                   let logger = Logger::root(slog::Discard, o!());
                   HalState::new(self.window, self.image, name, num_quads, vsync, logger)
               }

               
               pub fn build_logged(self, logger: Logger) -> Result<HalState, &'static str> {
                   use gfx_hal::window::PresentMode::*;
                   let num_quads = self.num_quads.unwrap_or(MAX_QUADS);
        let vsync = self
            .preferred_vsync
            .unwrap_or([Mailbox, Fifo, Relaxed, Immediate]);
        let name = self.name.unwrap_or("");
        HalState::new(self.window, self.image, name, num_quads, vsync, logger)
    }
}

pub struct HalState {
    num_quads: usize,
    vertices: BufferBundle<back::Backend, back::Device>,
    indexes: BufferBundle<back::Backend, back::Device>,
    texture: LoadedImage<back::Backend, back::Device>,
    descriptor_pool: ManuallyDrop<<back::Backend as Backend>::DescriptorPool>,
    descriptor_set: ManuallyDrop<<back::Backend as Backend>::DescriptorSet>,
    logger: Logger,
    descriptor_set_layouts: Vec<<back::Backend as Backend>::DescriptorSetLayout>,
    pipeline_layout: ManuallyDrop<<back::Backend as Backend>::PipelineLayout>,
    graphics_pipeline: ManuallyDrop<<back::Backend as Backend>::GraphicsPipeline>,
    current_frame: usize,
    frames_in_flight: usize,
    in_flight_fences: Vec<<back::Backend as Backend>::Fence>,
    render_finished_semaphores: Vec<<back::Backend as Backend>::Semaphore>,
    image_available_semaphores: Vec<<back::Backend as Backend>::Semaphore>,
    command_buffers: Vec<CommandBuffer<back::Backend, Graphics, MultiShot, Primary>>,
    command_pool: ManuallyDrop<CommandPool<back::Backend, Graphics>>,
    framebuffers: Vec<<back::Backend as Backend>::Framebuffer>,
    image_views: Vec<(<back::Backend as Backend>::ImageView)>,
    render_pass: ManuallyDrop<<back::Backend as Backend>::RenderPass>,
    render_area: Rect,
    queue_group: QueueGroup<back::Backend, Graphics>,
    swapchain: ManuallyDrop<<back::Backend as Backend>::Swapchain>,
    device: ManuallyDrop<back::Device>,
    _adapter: Adapter<back::Backend>,
    _surface: <back::Backend as Backend>::Surface,
    _instance: ManuallyDrop<back::Instance>,
}

impl std::fmt::Debug for HalState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "HalState  {{ /* stuff */ }} ")
    }
}

impl HalState {
    pub fn new(
        window: &winit::Window,
        texture: &[u8],
        name: &str,
        num_quads: usize,
        preferred_vsync: [PresentMode; 4],
        logger: slog::Logger,
    ) -> Result<Self, &'static str> {
        let instance = back::Instance::create(name, 1);
        let mut surface = instance.create_surface(window);
        let adapter = instance
            .enumerate_adapters()
            .into_iter()
            .find(|a| {
                a.queue_families
                    .iter()
                    .any(|qf| qf.supports_graphics() && surface.supports_queue_family(qf))
            })
            .ok_or("Couldn't find a graphical Adapter!")?;
        let (mut device, mut queue_group) = {
            let queue_family = adapter
                .queue_families
                .iter()
                .find(|qf| qf.supports_graphics() && surface.supports_queue_family(qf))
                .ok_or("Couldn't find QueueFamily with graphics!")?;
            let Gpu { device, mut queues } = unsafe {
                adapter
                    .physical_device
                    .open(&[(&queue_family, &[1.0; 1])])
                    .map_err(|_| "Couldn't open the PhysicalDevice!")?
            };
            let queue_group = queues
                .take::<Graphics>(queue_family.id())
                .ok_or("Couldn't take ownership of the QueueGroup!")?;
            let _ = if queue_group.queues.len() > 0 {
                Ok(())
            } else {
                Err("The QueueGroup didn't have any CommandQueues available!")
            }?;
            (device, queue_group)
        };

        let (swapchain, extent, backbuffer, format, frames_in_flight) = {
            let (caps, preferred_formats, present_modes, composite_alphas) =
                surface.compatibility(&adapter.physical_device);
            info!(&logger, "surface compatibility";
                  kv!("caps" => debug_repr!(caps),
                      "preferred_formats" => debug_repr!(preferred_formats),
                      "present_modes" => debug_repr!(present_modes),
                      "composite_alphas" => debug_repr!(composite_alphas)));
            //
            let present_mode = {
                preferred_vsync
                    .iter()
                    .cloned()
                    .find(|pm| present_modes.contains(pm))
                    .ok_or("No PresentMode values specified!")?
            };
            let composite_alpha = {
                use gfx_hal::window::CompositeAlpha::*;
                [Opaque, Inherit, PreMultiplied, PostMultiplied]
                    .iter()
                    .cloned()
                    .find(|ca| composite_alphas.contains(ca))
                    .ok_or("No CompositeAlpha values specified!")?
            };
            let format = match preferred_formats {
                None => Format::Rgba8Srgb,
                Some(formats) => match formats
                    .iter()
                    .find(|format| format.base_format().1 == ChannelType::Srgb)
                    .cloned()
                {
                    Some(srgb_format) => srgb_format,
                    None => formats
                        .get(0)
                        .cloned()
                        .ok_or("Preferred format list was empty!")?,
                },
            };
            // This really just grabs the extent as reported, but does some extra math since metal might report 4096x4096 because reasons
            let extent = {
                let window_client_area = window
                    .get_inner_size()
                    .ok_or("Window doesn't exist!")?
                    .to_physical(window.get_hidpi_factor());
                Extent2D {
                    width: caps.extents.end.width.min(window_client_area.width as u32),
                    height: caps
                        .extents
                        .end
                        .height
                        .min(window_client_area.height as u32),
                }
            };
            let image_count = if present_mode == PresentMode::Mailbox {
                (caps.image_count.end - 1).min(3)
            } else {
                (caps.image_count.end - 1).min(2)
            };
            let image_layers = 1;
            let image_usage = if caps.usage.contains(Usage::COLOR_ATTACHMENT) {
                Usage::COLOR_ATTACHMENT
            } else {
                Err("The surface isn't capable of supporting color!")?
            };
            let swapchain_config = SwapchainConfig {
                present_mode,
                composite_alpha,
                format,
                extent,
                image_count,
                image_layers,
                image_usage,
            };
            info!(logger, "created a swapchain config"; "swapchain_config" => format!("{:#?}", swapchain_config));
            let (swapchain, backbuffer) = unsafe {
                device
                    .create_swapchain(&mut surface, swapchain_config, None)
                    .map_err(|_| "Failed to create the swapchain!")?
            };
            (swapchain, extent, backbuffer, format, image_count as usize)
        };

        let (image_available_semaphores, render_finished_semaphores, in_flight_fences) = {
            let mut image_available_semaphores: Vec<<back::Backend as Backend>::Semaphore> = vec![];
            let mut render_finished_semaphores: Vec<<back::Backend as Backend>::Semaphore> = vec![];
            let mut in_flight_fences: Vec<<back::Backend as Backend>::Fence> = vec![];
            for _ in 0..frames_in_flight {
                in_flight_fences.push(
                    device
                        .create_fence(true)
                        .map_err(|_| "Could not create a fence!")?,
                );
                image_available_semaphores.push(
                    device
                        .create_semaphore()
                        .map_err(|_| "Could not create a semaphore!")?,
                );
                render_finished_semaphores.push(
                    device
                        .create_semaphore()
                        .map_err(|_| "Could not create a semaphore!")?,
                );
            }
            (
                image_available_semaphores,
                render_finished_semaphores,
                in_flight_fences,
            )
        };
        let render_pass = {
            let color_attachment = Attachment {
                format: Some(format),
                samples: 1,
                ops: AttachmentOps {
                    load: AttachmentLoadOp::Clear,
                    store: AttachmentStoreOp::Store,
                },
                stencil_ops: AttachmentOps::DONT_CARE,
                layouts: Layout::Undefined..Layout::Present,
            };
            let subpass = SubpassDesc {
                colors: &[(0, Layout::ColorAttachmentOptimal)],
                depth_stencil: None,
                inputs: &[],
                resolves: &[],
                preserves: &[],
            };
            unsafe {
                device
                    .create_render_pass(&[color_attachment], &[subpass], &[])
                    .map_err(|_| "Couldn't create a render pass!")?
            }
        };
        let image_views: Vec<_> = match backbuffer {
            Backbuffer::Images(images) => images
                .into_iter()
                .map(|image| unsafe {
                    device
                        .create_image_view(
                            &image,
                            ViewKind::D2,
                            format,
                            Swizzle::NO,
                            SubresourceRange {
                                aspects: Aspects::COLOR,
                                levels: 0..1,
                                layers: 0..1,
                            },
                        )
                        .map_err(|_| "Couldn't create the image view for the image!")
                })
                .collect::<Result<Vec<_>, &str>>()?,
            Backbuffer::Framebuffer(_) => unimplemented!("Can't handle framebuffer backbuffer!"),
        };
        let framebuffers: Vec<<back::Backend as Backend>::Framebuffer> = {
            image_views
                .iter()
                .map(|image_view| unsafe {
                    device
                        .create_framebuffer(
                            &render_pass,
                            vec![image_view],
                            Extent {
                                width: extent.width as u32,
                                height: extent.height as u32,
                                depth: 1,
                            },
                        )
                        .map_err(|_| "Failed to create a framebuffer!")
                })
                .collect::<Result<Vec<_>, &str>>()?
        };
        let mut command_pool = unsafe {
            device
                .create_command_pool_typed(&queue_group, CommandPoolCreateFlags::RESET_INDIVIDUAL)
                .map_err(|_| "Couldn't create the raw command pool!")?
        };

        let command_buffers: Vec<_> = framebuffers
            .iter()
            .map(|_| command_pool.acquire_command_buffer())
            .collect();

        let (descriptor_set_layouts, pipeline_layout, graphics_pipeline) =
            Self::create_pipeline(&mut device, extent, &render_pass, &logger)?;

        // 2. you create a descriptor pool, and when making that descriptor pool
        //    you specify how many sets you want to be able to allocate from the
        //    pool, as well as the maximum number of each kind of descriptor you
        //    want to be able to allocate from that pool, total, for all sets.
        let mut descriptor_pool = unsafe {
            device
                .create_descriptor_pool(
                    1, // sets
                    &[
                        gfx_hal::pso::DescriptorRangeDesc {
                            ty: gfx_hal::pso::DescriptorType::SampledImage,
                            count: 1,
                        },
                        gfx_hal::pso::DescriptorRangeDesc {
                            ty: gfx_hal::pso::DescriptorType::Sampler,
                            count: 1,
                        },
                    ],
                )
                .map_err(|_| "Couldn't create a descriptor pool!")?
        };
        // 3. you allocate said descriptor set from the pool you made earlier
        let descriptor_set = unsafe {
            descriptor_pool
                .allocate_set(&descriptor_set_layouts[0])
                .map_err(|_| "Couldn't make a descriptor set!")?
        };

        // 4. You create the actual descriptors which you want to write into the
        //    allocated descriptor set (in this case an image and a sampler) (see 1-3 in create_pipeline)
        let texture = LoadedImage::new(
            &adapter,
            &device,
            &mut command_pool,
            &mut queue_group.queues[0],
            image::load_from_memory(texture)
                .expect("Binary corrupted!")
                .to_rgba(),
        )?;

        // 5. You write the descriptors into the descriptor set using
        //    write_descriptor_sets which you pass a set of DescriptorSetWrites
        //    which each write in one or more descriptors to the set
        unsafe {
            device.write_descriptor_sets(vec![
                gfx_hal::pso::DescriptorSetWrite {
                    set: &descriptor_set,
                    binding: 0,
                    array_offset: 0,
                    descriptors: Some(gfx_hal::pso::Descriptor::Image(
                        texture.image_view.deref(),
                        Layout::Undefined,
                    )),
                },
                gfx_hal::pso::DescriptorSetWrite {
                    set: &descriptor_set,
                    binding: 1,
                    array_offset: 0,
                    descriptors: Some(gfx_hal::pso::Descriptor::Sampler(texture.sampler.deref())),
                },
            ]);
        }
        // 6. You actually bind the descriptor set in the command buffer before
        //    the draw call using bind_graphics_descriptor_sets

        const F32_XY_RGB_UV_UVRECT_QUAD: usize = mem::size_of::<Vertex>() * 4;
        let vertices = BufferBundle::new(
            &adapter,
            &device,
            F32_XY_RGB_UV_UVRECT_QUAD * num_quads,
            BufferUsage::VERTEX,
        )?;
        const U16_QUAD_INDICES: usize = mem::size_of::<u16>() * 2 * 3;
        let indexes = BufferBundle::new(
            &adapter,
            &device,
            U16_QUAD_INDICES * num_quads,
            BufferUsage::INDEX,
        )?;

        unsafe {
            let mut data_target = device
                .acquire_mapping_writer(&indexes.memory, 0..indexes.requirements.size)
                .map_err(|_| "Failed to require an index buffer mapping writer!")?;
            const INDEX_DATA: &[u16] = &[0, 1, 2, 2, 3, 0];
            for i in 0..num_quads {
                let stride: usize = 6;
                let vertex_stride = 4;
                let index_data: &[u16] = &[
                    i as u16 * vertex_stride + INDEX_DATA[0],
                    i as u16 * vertex_stride + INDEX_DATA[1],
                    i as u16 * vertex_stride + INDEX_DATA[2],
                    i as u16 * vertex_stride + INDEX_DATA[3],
                    i as u16 * vertex_stride + INDEX_DATA[4],
                    i as u16 * vertex_stride + INDEX_DATA[5],
                ];
                data_target[stride * i..stride * (i + 1)].copy_from_slice(&index_data);
            }
            device
                .release_mapping_writer(data_target)
                .map_err(|_| "Couldn't release the index buffer mapping writer!")?;
        }

        Ok(HalState {
            num_quads,
            vertices,
            indexes,
            texture,
            descriptor_pool: ManuallyDrop::new(descriptor_pool),
            descriptor_set: ManuallyDrop::new(descriptor_set),
            logger,
            current_frame: 0,
            frames_in_flight,
            in_flight_fences,
            render_finished_semaphores,
            image_available_semaphores,
            command_buffers,
            command_pool: ManuallyDrop::new(command_pool),
            framebuffers,
            image_views,
            render_pass: ManuallyDrop::new(render_pass),
            render_area: extent.to_extent().rect(),
            queue_group,
            swapchain: ManuallyDrop::new(swapchain),
            device: ManuallyDrop::new(device),
            descriptor_set_layouts,
            pipeline_layout: ManuallyDrop::new(pipeline_layout),
            graphics_pipeline: ManuallyDrop::new(graphics_pipeline),
            _adapter: adapter,
            _surface: surface,
            _instance: ManuallyDrop::new(instance),
        })
    }

    pub fn extend_quad_alloc(&mut self, new_max: usize) -> Result<(), &'static str> {
        
        const F32_XY_RGB_UV_UVRECT_QUAD: usize = mem::size_of::<Vertex>() * 4;
        if new_max as u64 > self.vertices.requirements.size / F32_XY_RGB_UV_UVRECT_QUAD as u64 {
            info!(&self.logger, "extending quad vertex/index buffer size"; "new_size" => new_max);

            unsafe {
                let new_vertices = BufferBundle::new(
                    &self._adapter,
                    &*self.device,
                    F32_XY_RGB_UV_UVRECT_QUAD * new_max,
                    BufferUsage::VERTEX,
                )?;
                const U16_QUAD_INDICES: usize = mem::size_of::<u16>() * 2 * 3;
                let new_indexes = {
                    let res = BufferBundle::new(
                        &self._adapter,
                        self.device.deref(),
                        U16_QUAD_INDICES * new_max,
                        BufferUsage::INDEX,
                    );
                    if res.is_err() {
                        new_vertices.manually_drop(&self.device);
                    }
                    res?
                };
                let mut data_target = {
                    let res = self
                        .device
                        .acquire_mapping_writer(
                            &new_indexes.memory,
                            0..new_indexes.requirements.size,
                        )
                        .map_err(|_| "Failed to require an index buffer mapping writer!");
                    if res.is_err() {
                        new_vertices.manually_drop(&self.device);
                        new_indexes.manually_drop(&self.device);
                    }
                    res?
                };
                const INDEX_DATA: &[u16] = &[0, 1, 2, 2, 3, 0];
                for i in 0..new_max {
                    let stride: usize = 6;
                    let vertex_stride = 4;
                    let index_data: &[u16] = &[
                        i as u16 * vertex_stride + INDEX_DATA[0],
                        i as u16 * vertex_stride + INDEX_DATA[1],
                        i as u16 * vertex_stride + INDEX_DATA[2],
                        i as u16 * vertex_stride + INDEX_DATA[3],
                        i as u16 * vertex_stride + INDEX_DATA[4],
                        i as u16 * vertex_stride + INDEX_DATA[5],
                    ];
                    data_target[stride * i..stride * (i + 1)].copy_from_slice(&index_data);
                }
                if let Err(_) = self.device.release_mapping_writer(data_target) {
                    new_vertices.manually_drop(&self.device);
                        new_indexes.manually_drop(&self.device);
                        return Err("Couldn't release the index buffer mapping writer!");
                    }
                    let old_vertex_buffer = mem::replace(&mut self.vertices, new_vertices);
                    let old_index_buffer = mem::replace(&mut self.indexes, new_indexes);
                    old_vertex_buffer.manually_drop(&self.device);
                    old_index_buffer.manually_drop(&self.device);
                    self.num_quads = new_max;
                }
        }
        Ok(())
    }

    pub fn draw_clear_frame(&mut self, color: [f32; 4]) -> Result<(), &'static str> {
        // FRAME SETUP
        let image_available = &self.image_available_semaphores[self.current_frame];
        let render_finished = &self.render_finished_semaphores[self.current_frame];

        // advance the frame before early returns can happen
        self.current_frame = (self.current_frame + 1) % self.frames_in_flight;

        let (i_u32, i_usize) = unsafe {
            let image_index = self
                .swapchain
                .acquire_image(core::u64::MAX, FrameSync::Semaphore(image_available))
                .map_err(|_| "Couldn't acquire an image from the swapchain!")?;
            (image_index, image_index as usize)
        };

        let flight_fence = &self.in_flight_fences[i_usize];
        unsafe {
            self.device
                .wait_for_fence(flight_fence, core::u64::MAX)
                .map_err(|_| "Failed to wait on the fence!")?;
            self.device
                .reset_fence(flight_fence)
                .map_err(|_| "Couldn't reset fence!")?;
        }

        // Record commands
        unsafe {
            let buffer = &mut self.command_buffers[i_usize];
            let clear_values = [ClearValue::Color(ClearColor::Float(color))];
            buffer.begin(false);
            buffer.begin_render_pass_inline(
                &self.render_pass,
                &self.framebuffers[i_usize],
                self.render_area,
                clear_values.iter(),
            );
            buffer.finish();
        }

        // Submission
        let command_buffers = &self.command_buffers[i_usize..=i_usize];
        let wait_semaphores: ArrayVec<[_; 1]> =
            [(image_available, PipelineStage::COLOR_ATTACHMENT_OUTPUT)].into();
        let signal_semaphores: ArrayVec<[_; 1]> = [render_finished].into();
        // apparently, you gotta do this twice, because reasons
        let present_wait_semaphores: ArrayVec<[_; 1]> = [render_finished].into();
        let submission = Submission {
            command_buffers,
            wait_semaphores,
            signal_semaphores,
        };
        let the_command_queue = &mut self.queue_group.queues[0];
        unsafe {
            the_command_queue.submit(submission, Some(flight_fence));
            self.swapchain
                .present(the_command_queue, i_u32, present_wait_semaphores)
                .map_err(|_| "Failed to present into the swapchain!")
        }
    }

    pub fn draw_quad_frame(&mut self, textured_quads: &[TexturedQuad]) -> Result<(), &'static str> {
        // advance the frame before early returns can happen
        self.current_frame = (self.current_frame + 1) % self.frames_in_flight;

        if self.num_quads <= textured_quads.len() {
            self.extend_quad_alloc(textured_quads.len())?;
        }

        // FRAME SETUP
        let image_available = &self.image_available_semaphores[self.current_frame];
        let render_finished = &self.render_finished_semaphores[self.current_frame];

        let (i_u32, i_usize) = unsafe {
            let image_index = self
                .swapchain
                .acquire_image(core::u64::MAX, FrameSync::Semaphore(image_available))
                .map_err(|_| "Couldn't acquire an image from the swapchain!")?;
            (image_index, image_index as usize)
        };

        let flight_fence = &self.in_flight_fences[i_usize];
        unsafe {
            self.device
                .wait_for_fence(flight_fence, core::u64::MAX)
                .map_err(|_| "Failed to wait on the fence!")?;
            self.device
                .reset_fence(flight_fence)
                .map_err(|_| "Couldn't reset fence!")?;
        }

        unsafe {
            let mut data_target = self
                .device
                .acquire_mapping_writer(
                    self.vertices.memory.deref(),
                    0..self.vertices.requirements.size,
                )
                .map_err(|_| "Failed to acquire a memory writer!")?;
            for i in 0..textured_quads.len().min(MAX_QUADS) {
                let stride = (2 + 3 + 2 + 4) * 4;
                data_target[stride * i..stride * (i + 1)]
                    .copy_from_slice(&textured_quads[i].to_f32s());
            }
            self.device
                .release_mapping_writer(data_target)
                .map_err(|_| "Couldn't release the mapping writer")?;
        }

        let uv_rect = textured_quads[0].uv_rect;
        // record commands
        unsafe {
            let buffer = &mut self.command_buffers[i_usize];
            const TRIANGLE_CLEAR: [ClearValue; 1] =
                [ClearValue::Color(ClearColor::Float([0.1, 0.2, 0.3, 1.0]))];
            buffer.begin(false);
            {
                let mut encoder = buffer.begin_render_pass_inline(
                    &self.render_pass,
                    &self.framebuffers[i_usize],
                    self.render_area,
                    TRIANGLE_CLEAR.iter(),
                );
                encoder.bind_graphics_pipeline(&self.graphics_pipeline);
                // force deref impl of ManuallyDrop to do stuff
                let buffer_ref: &<back::Backend as Backend>::Buffer = &self.vertices.buffer;
                let buffers: ArrayVec<[_; 1]> = [(buffer_ref, 0)].into();
                encoder.bind_vertex_buffers(0, buffers);
                encoder.bind_index_buffer(IndexBufferView {
                    buffer: &self.indexes.buffer,
                    offset: 0,
                    index_type: IndexType::U16,
                });
                encoder.bind_graphics_descriptor_sets(
                    &self.pipeline_layout,
                    0,
                    Some(self.descriptor_set.deref()),
                    &[],
                );
                encoder.push_graphics_constants(
                    &self.pipeline_layout,
                    ShaderStageFlags::VERTEX,
                    0,
                    &[
                        uv_rect[0].to_bits(),
                        uv_rect[1].to_bits(),
                        uv_rect[2].to_bits(),
                        uv_rect[3].to_bits(),
                    ],
                );
                encoder.draw_indexed(0..6 * textured_quads.len() as u32, 0, 0..1);
            }
            buffer.finish()
        }

        // Submission
        let command_buffers = &self.command_buffers[i_usize..=i_usize];
        let wait_semaphores: ArrayVec<[_; 1]> =
            [(image_available, PipelineStage::COLOR_ATTACHMENT_OUTPUT)].into();
        let signal_semaphores: ArrayVec<[_; 1]> = [render_finished].into();
        // apparently, you gotta do this twice, because reasons
        let present_wait_semaphores: ArrayVec<[_; 1]> = [render_finished].into();
        let submission = Submission {
            command_buffers,
            wait_semaphores,
            signal_semaphores,
        };
        let the_command_queue = &mut self.queue_group.queues[0];
        unsafe {
            the_command_queue.submit(submission, Some(flight_fence));
            self.swapchain
                .present(the_command_queue, i_u32, present_wait_semaphores)
                .map_err(|_| "Failed to present into the swapchain!")
        }
    }
    
    fn create_pipeline(
        device: &mut back::Device,
        extent: Extent2D,
        render_pass: &<back::Backend as Backend>::RenderPass,
        logger: &Logger,
    ) -> Result<
            (
                Vec<<back::Backend as Backend>::DescriptorSetLayout>,
                <back::Backend as Backend>::PipelineLayout,
                <back::Backend as Backend>::GraphicsPipeline,
            ),
        &'static str,
        > {
        let mut compiler = shaderc::Compiler::new().ok_or("shaderc not found!")?;
        let vertex_compile_artifact = compiler
            .compile_into_spirv(
                VERTEX_SOURCE,
                shaderc::ShaderKind::Vertex,
                "vertex.vert",
                "halstate",
                None,
            )
            .map_err(|e| {
                error!(logger, "failed to compile vertex shader"; "err" => %e);
                "Couldn't compile vertex shader!"
            })?;
        let fragment_compile_artifact = compiler
            .compile_into_spirv(
                FRAGMENT_SOURCE,
                shaderc::ShaderKind::Fragment,
                "fragment.frag",
                "halstate",
                None,
            )
            .map_err(|e| {
                error!(logger, "failed to compile fragment shader"; "err" => %e);
                "Couldn't compile fragment shader!"
            })?;
        let vertex_shader_module = unsafe {
            device
                .create_shader_module(vertex_compile_artifact.as_binary_u8())
                .map_err(|_| "Couldn't make the vertex module!")?
        };
        let fragment_shader_module = unsafe {
            device
                .create_shader_module(fragment_compile_artifact.as_binary_u8())
                .map_err(|_| "Couldn't make the fragment module!")?
        };
        let shaders = {
            let (vs_entry, fs_entry) = (
                EntryPoint {
                    entry: "main",
                    module: &vertex_shader_module,
                    specialization: Specialization {
                        constants: &[],
                        data: &[],
                    },
                },
                EntryPoint {
                    entry: "main",
                    module: &fragment_shader_module,
                    specialization: Specialization {
                        constants: &[],
                        data: &[],
                    },
                },
            );
            GraphicsShaderSet {
                vertex: vs_entry,
                hull: None,
                domain: None,
                geometry: None,
                fragment: Some(fs_entry),
            }
        };
        let vertex_buffers: Vec<VertexBufferDesc> = vec![VertexBufferDesc {
            binding: 0,
            stride: (mem::size_of::<f32>() * (2 + 3 + 2 + 4)) as ElemStride,
            rate: 0,
        }];

        let attributes: Vec<AttributeDesc> = Vertex::attributes();

        let rasterizer = Rasterizer {
            depth_clamping: false,
            polygon_mode: PolygonMode::Fill,
            cull_face: Face::NONE,
            front_face: FrontFace::Clockwise,
            depth_bias: None,
            conservative: false,
        };
        let depth_stencil = DepthStencilDesc {
            depth: DepthTest::Off,
            depth_bounds: false,
            stencil: StencilTest::Off,
        };
        let blender = {
            // stuff that we were using before but yeah
            /* let blend_state = BlendState::On {
            color: BlendOp::Add {
            src: Factor::One,
            dst: Factor::Zero,
        },
            alpha: BlendOp::Add {
            src: Factor::One,
            dst: Factor::Zero,
        },
        };*/
            BlendDesc {
                logic_op: Some(LogicOp::Copy),
                targets: vec![ColorBlendDesc(ColorMask::ALL, BlendState::ALPHA)],
            }
        };
        let baked_states = BakedStates {
            viewport: Some(Viewport {
                rect: extent.to_extent().rect(),
                depth: (0.0..1.0),
            }),
            scissor: Some(extent.to_extent().rect()),
            blend_color: None,
            depth_bounds: None,
        };
        let input_assembler = InputAssemblerDesc::new(Primitive::TriangleList);
        // Apparently these variables are unused, but yeah, gonna keep them as comments here just in case
        // let bindings = Vec::<DescriptorSetLayoutBinding>::new();
        // let immutable_samplers = Vec::<<back::Backend as Backend>::Sampler>::new();
        
        // 1. you make a DescriptorSetLayout which is the layout of one descriptor
        //    set
        let descriptor_set_layouts: Vec<<back::Backend as Backend>::DescriptorSetLayout> =
            vec![unsafe {
                device
                    .create_descriptor_set_layout(
                        &[
                            DescriptorSetLayoutBinding {
                                binding: 0,
                                ty: gfx_hal::pso::DescriptorType::SampledImage,
                                count: 1,
                                stage_flags: ShaderStageFlags::FRAGMENT | ShaderStageFlags::VERTEX,
                                immutable_samplers: false,
                            },
                            DescriptorSetLayoutBinding {
                                binding: 1,
                                ty: gfx_hal::pso::DescriptorType::Sampler,
                                count: 1,
                                stage_flags: ShaderStageFlags::FRAGMENT | ShaderStageFlags::VERTEX,
                                immutable_samplers: false,
                            },
                        ],
                        &[],
                    )
                    .map_err(|_| "Couldn't make a DescriptorSetLayout")?
            }];

        let push_constants = vec![(ShaderStageFlags::VERTEX, 0..5)];
        let layout = unsafe {
            device
                .create_pipeline_layout(&descriptor_set_layouts, push_constants)
                .map_err(|_| "Couldn't create pipeline layout!")?
        };
        let gfx_pipeline = {
            let desc = GraphicsPipelineDesc {
                shaders,
                rasterizer,
                vertex_buffers,
                attributes,
                input_assembler,
                blender,
                depth_stencil,
                layout: &layout,
                multisampling: None,
                baked_states,
                subpass: Subpass {
                    index: 0,
                    main_pass: render_pass,
                },
                flags: PipelineCreationFlags::empty(),
                parent: BasePipeline::None,
            };

            unsafe {
                device
                    .create_graphics_pipeline(&desc, None)
                    .map_err(|_| "Couldn't create graphics pipeline!")?
            }
        };
        Ok((descriptor_set_layouts, layout, gfx_pipeline))
    }
}

impl core::ops::Drop for HalState {
    fn drop(&mut self) {
        use core::ptr::read;
        let _ = self.device.wait_idle();
        unsafe {
            for descriptor_set_layout in self.descriptor_set_layouts.drain(..) {
                self.device
                    .destroy_descriptor_set_layout(descriptor_set_layout);
            }
            for in_flight_fence in self.in_flight_fences.drain(..) {
                self.device.destroy_fence(in_flight_fence);
            }
            for render_finished_semaphore in self.render_finished_semaphores.drain(..) {
                self.device.destroy_semaphore(render_finished_semaphore);
            }
            for image_available_semaphore in self.image_available_semaphores.drain(..) {
                self.device.destroy_semaphore(image_available_semaphore);
            }
            for framebuffer in self.framebuffers.drain(..) {
                self.device.destroy_framebuffer(framebuffer);
            }
            for image_view in self.image_views.drain(..) {
                self.device.destroy_image_view(image_view);
            }
            self.vertices.manually_drop(self.device.deref());
            self.indexes.manually_drop(self.device.deref());
            self.texture.manually_drop(self.device.deref());
            self.device
                .destroy_descriptor_pool(ManuallyDrop::into_inner(read(&self.descriptor_pool)));
            self.device
                .destroy_pipeline_layout(ManuallyDrop::into_inner(read(&self.pipeline_layout)));
            self.device
                .destroy_graphics_pipeline(ManuallyDrop::into_inner(read(
                    &mut self.graphics_pipeline,
                )));
            self.device.destroy_command_pool(
                ManuallyDrop::into_inner(read(&self.command_pool)).into_raw(),
            );
            self.device
                .destroy_render_pass(ManuallyDrop::into_inner(read(&mut self.render_pass)));
            self.device
                .destroy_swapchain(ManuallyDrop::into_inner(read(&mut self.swapchain)));
            ManuallyDrop::drop(&mut self.device);
            ManuallyDrop::drop(&mut self._instance);
        }
    }
}
