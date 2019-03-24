extern crate shaderc;

#[cfg(feature = "dx12")]
use gfx_backend_dx12 as back;
#[cfg(feature = "metal")]
use gfx_backend_metal as back;
#[cfg(feature = "vulkan")]
use gfx_backend_vulkan as back;

use crate::geometry::{Point2D, Triangle};
use arrayvec::ArrayVec;
use core::{
    marker::PhantomData,
    mem::{self, ManuallyDrop},
    ops::Deref,
};
use gfx_hal::{
    adapter::{Adapter, MemoryTypeId, PhysicalDevice},
    buffer::{IndexBufferView, Usage as BufferUsage},
    command::{ClearColor, ClearValue, CommandBuffer, MultiShot, Primary},
    device::Device,
    format::{Aspects, ChannelType, Format, Swizzle},
    image::{Extent, Layout, SubresourceRange, Usage, ViewKind},
    memory::{Properties, Requirements},
    pass::{Attachment, AttachmentLoadOp, AttachmentOps, AttachmentStoreOp, Subpass, SubpassDesc},
    pool::{CommandPool, CommandPoolCreateFlags},
    pso::{
        AttributeDesc, BakedStates, BasePipeline, BlendDesc, BlendOp, BlendState, ColorBlendDesc,
        ColorMask, DepthStencilDesc, DepthTest, DescriptorSetLayoutBinding, ElemOffset, ElemStride,
        Element, EntryPoint, Face, Factor, FrontFace, GraphicsPipelineDesc, GraphicsShaderSet,
        InputAssemblerDesc, LogicOp, PipelineCreationFlags, PipelineStage, PolygonMode, Rasterizer,
        Rect, ShaderStageFlags, Specialization, StencilTest, VertexBufferDesc, Viewport,
    },
    queue::{
        capability::{Capability, Supports, Transfer},
        family::QueueGroup,
        CommandQueue, Submission,
    },
    window::{Backbuffer, Extent2D, FrameSync, PresentMode, Swapchain, SwapchainConfig},
    Backend, DescriptorPool, Gpu, Graphics, IndexType, Instance, Primitive, QueueFamily, Surface,
};
use std::time::Instant;

const VERTEX_SOURCE: &str = include_str!("vertex.glsl");
const FRAGMENT_SOURCE: &str = include_str!("fragment.glsl");
static CREATURE_BYTES: &[u8] = include_bytes!("creature-smol.png");

pub struct BufferBundle<B: Backend, D: Device<B>> {
    pub buffer: ManuallyDrop<B::Buffer>,
    pub requirements: Requirements,
    pub memory: ManuallyDrop<B::Memory>,
    pub phantom: PhantomData<D>,
}

pub struct LoadedImage<B: Backend, D: Device<B>> {
    pub image: ManuallyDrop<B::Image>,
    pub requirements: Requirements,
    pub memory: ManuallyDrop<B::Memory>,
    pub image_view: ManuallyDrop<B::ImageView>,
    pub sampler: ManuallyDrop<B::Sampler>,
    pub phantom: PhantomData<D>,
}

impl<B: Backend, D: Device<B>> BufferBundle<B, D> {
    pub fn new(
        adapter: &Adapter<B>,
        device: &D,
        size: usize,
        usage: BufferUsage,
    ) -> Result<Self, &'static str> {
        unsafe {
            let mut buffer = device
                .create_buffer(size as u64, usage)
                .map_err(|_| "Couldn't create a buffer!")?;
            let requirements = device.get_buffer_requirements(&buffer);
            let memory_type_id = adapter
                .physical_device
                .memory_properties()
                .memory_types
                .iter()
                .enumerate()
                .find(|&(id, memory_type)| {
                    requirements.type_mask & (1 << id) != 0
                        && memory_type.properties.contains(Properties::CPU_VISIBLE)
                })
                .map(|(id, _)| MemoryTypeId(id))
                .ok_or("Couldn't find a memory type to support the vertex buffer")?;
            let memory = device
                .allocate_memory(memory_type_id, requirements.size)
                .map_err(|_| "Couldn't allocate buffer memory!")?;
            device
                .bind_buffer_memory(&memory, 0, &mut buffer)
                .map_err(|_| "Couldn't bind the buffer memory!")?;
            Ok(BufferBundle {
                buffer: ManuallyDrop::new(buffer),
                requirements,
                memory: ManuallyDrop::new(memory),
                phantom: PhantomData,
            })
        }
    }

    pub unsafe fn manually_drop(&self, device: &D) {
        use core::ptr::read;
        device.destroy_buffer(ManuallyDrop::into_inner(read(&self.buffer)));
        device.free_memory(ManuallyDrop::into_inner(read(&self.memory)));
    }
}

impl<B: Backend, D: Device<B>> LoadedImage<B, D> {
    pub fn new<C: Capability + Supports<Transfer>>(
        adapter: &Adapter<B>,
        device: &D,
        command_pool: &mut CommandPool<B, C>,
        command_queue: &mut CommandQueue<B, C>,
        img: image::RgbaImage,
    ) -> Result<Self, &'static str> {
        unsafe {
            let pixel_size = mem::size_of::<image::Rgba<u8>>();
            let row_size = pixel_size * (img.width() as usize);
            let limits = adapter.physical_device.limits();
            let row_alignment_mask = limits.min_buffer_copy_pitch_alignment as u32 - 1;
            let row_pitch = ((row_size as u32 + row_alignment_mask) & !row_alignment_mask) as usize;
            debug_assert!(row_pitch as usize >= row_size);

            // 1. make a staging buffer with enough memory for the image, and a
            //    transfer_src usage
            let required_bytes = row_pitch * img.height() as usize;
            let staging_bundle =
                BufferBundle::new(&adapter, device, required_bytes, BufferUsage::TRANSFER_SRC)?;

            // 2. use mapping writer to put the image data into that buffer
            let mut writer = device
                .acquire_mapping_writer::<u8>(
                    &staging_bundle.memory,
                    0..staging_bundle.requirements.size,
                )
                .map_err(|_| "Failed to acquire a mapping writer to the staging buffer!")?;
            for y in 0..img.height() as usize {
                let row = &(*img)[y * row_size..(y + 1) * row_size];
                let dest_base = y * row_pitch;
                writer[dest_base..dest_base + row.len()].copy_from_slice(row);
            }
            device
                .release_mapping_writer(writer)
                .map_err(|_| "Couldn't release the mapping writer to the staging buffer!")?;

            // 3. Make an image with transfer_dst and SAMPLED usage
            let mut the_image = device
                .create_image(
                    gfx_hal::image::Kind::D2(img.width(), img.height(), 1, 1),
                    1,
                    Format::Rgba8Srgb,
                    gfx_hal::image::Tiling::Optimal,
                    gfx_hal::image::Usage::TRANSFER_DST | gfx_hal::image::Usage::SAMPLED,
                    gfx_hal::image::ViewCapabilities::empty(),
                )
                .map_err(|_| "Couldn't create the image!")?;

            // 4. allocate memory for the image and bind it
            let requirements = device.get_image_requirements(&the_image);
            let memory_type_id = adapter
                .physical_device
                .memory_properties()
                .memory_types
                .iter()
                .enumerate()
                .find(|&(id, memory_type)| {
                    // BIG NOTE: THIS IS DEVICE LOCAL NOT CPU VISIBLE
                    requirements.type_mask & (1 << id) != 0
                        && memory_type.properties.contains(Properties::DEVICE_LOCAL)
                })
                .map(|(id, _)| MemoryTypeId(id))
                .ok_or("Couldn't find memory type to support the image!")?;
            let memory = device
                .allocate_memory(memory_type_id, requirements.size)
                .map_err(|_| "Couldn't allocate image memory!")?;
            device
                .bind_image_memory(&memory, 0, &mut the_image)
                .map_err(|_| "Couldn't bind the image memory!")?;

            // 5. create image view and sampler
            let image_view = device
                .create_image_view(
                    &the_image,
                    gfx_hal::image::ViewKind::D2,
                    Format::Rgba8Srgb,
                    gfx_hal::format::Swizzle::NO,
                    SubresourceRange {
                        aspects: Aspects::COLOR,
                        levels: 0..1,
                        layers: 0..1,
                    },
                )
                .map_err(|_| "Couldn't create the image view!")?;
            let sampler = device
                .create_sampler(gfx_hal::image::SamplerInfo::new(
                    gfx_hal::image::Filter::Nearest,
                    gfx_hal::image::WrapMode::Tile,
                ))
                .map_err(|_| "Couldn't create the sampler!")?;

            // 6. create a CommandBuffer
            let mut cmd_buffer = command_pool.acquire_command_buffer::<gfx_hal::command::OneShot>();
            cmd_buffer.begin();

            // 7. Use a pipeline barrier to transition the image from empty/undefined
            //    to TRANSFER_WRITE/TransferDstOptimal
            let image_barrier = gfx_hal::memory::Barrier::Image {
                states: (gfx_hal::image::Access::empty(), Layout::Undefined)
                    ..(
                        gfx_hal::image::Access::TRANSFER_WRITE,
                        Layout::TransferDstOptimal,
                    ),
                target: &the_image,
                families: None,
                range: SubresourceRange {
                    aspects: Aspects::COLOR,
                    levels: 0..1,
                    layers: 0..1,
                },
            };
            cmd_buffer.pipeline_barrier(
                PipelineStage::TOP_OF_PIPE..PipelineStage::TRANSFER,
                gfx_hal::memory::Dependencies::empty(),
                &[image_barrier],
            );

            // 8. perform copy from staging buffer to image
            cmd_buffer.copy_buffer_to_image(
                &staging_bundle.buffer,
                &the_image,
                Layout::TransferDstOptimal,
                &[gfx_hal::command::BufferImageCopy {
                    buffer_offset: 0,
                    buffer_width: (row_pitch / pixel_size) as u32,
                    buffer_height: img.height(),
                    image_layers: gfx_hal::image::SubresourceLayers {
                        aspects: Aspects::COLOR,
                        level: 0,
                        layers: 0..1,
                    },
                    image_offset: gfx_hal::image::Offset { x: 0, y: 0, z: 0 },
                    image_extent: gfx_hal::image::Extent {
                        width: img.width(),
                        height: img.height(),
                        depth: 1,
                    },
                }],
            );

            // 9. use pipeline barrier to transition the image to SHADER_READ access/
            //    ShaderReadOnlyOptimal layout
            let image_barrier = gfx_hal::memory::Barrier::Image {
                states: (
                    gfx_hal::image::Access::TRANSFER_WRITE,
                    Layout::TransferDstOptimal,
                )
                    ..(
                        gfx_hal::image::Access::SHADER_READ,
                        Layout::ShaderReadOnlyOptimal,
                    ),
                target: &the_image,
                families: None,
                range: SubresourceRange {
                    aspects: Aspects::COLOR,
                    levels: 0..1,
                    layers: 0..1,
                },
            };
            cmd_buffer.pipeline_barrier(
                PipelineStage::TRANSFER..PipelineStage::FRAGMENT_SHADER,
                gfx_hal::memory::Dependencies::empty(),
                &[image_barrier],
            );

            // 10. Submit the cmd buffer to queue and wait for it
            cmd_buffer.finish();
            let upload_fence = device
                .create_fence(false)
                .map_err(|_| "Couldn't create an upload fence!")?;
            command_queue.submit_nosemaphores(Some(&cmd_buffer), Some(&upload_fence));
            device
                .wait_for_fence(&upload_fence, core::u64::MAX)
                .map_err(|_| "Couldn't wait for the fence!")?;
            device.destroy_fence(upload_fence);

            // 11. Destroy the staging bundle and one shot buffer now that we're done
            staging_bundle.manually_drop(device);
            command_pool.free(Some(cmd_buffer));

            Ok(LoadedImage {
                image: ManuallyDrop::new(the_image),
                requirements,
                memory: ManuallyDrop::new(memory),
                image_view: ManuallyDrop::new(image_view),
                sampler: ManuallyDrop::new(sampler),
                phantom: PhantomData,
            })
        }
    }

    pub unsafe fn manually_drop(&self, device: &D) {
        use core::ptr::read;
        device.destroy_sampler(ManuallyDrop::into_inner(read(&self.sampler)));
        device.destroy_image_view(ManuallyDrop::into_inner(read(&self.image_view)));
        device.destroy_image(ManuallyDrop::into_inner(read(&self.image)));
        device.free_memory(ManuallyDrop::into_inner(read(&self.memory)));
    }
}

pub struct HalState {
    creation_instant: Instant,
    vertices: BufferBundle<back::Backend, back::Device>,
    indexes: BufferBundle<back::Backend, back::Device>,
    texture: LoadedImage<back::Backend, back::Device>,
    descriptor_pool: ManuallyDrop<<back::Backend as Backend>::DescriptorPool>,
    descriptor_set: ManuallyDrop<<back::Backend as Backend>::DescriptorSet>,
    logger: slog::Logger,
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

impl HalState {
    pub fn new(window: &crate::WindowState) -> Result<Self, &'static str> {
        let logger = window.logger.new(o!("window" => "halstate"));
        let instance = back::Instance::create(&window.window_name, 1);
        let mut surface = instance.create_surface(&window.window);
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
            info!(logger, "caps"; "caps" => format!("{:?}", caps));
            info!(logger, "preferred formats"; "preferred_formats" => format!("{:?}", preferred_formats));
            info!(logger, "present modes"; "present_modes" => format!("{:?}", present_modes));
            info!(logger, "composite alphas"; "composite_alphas" => format!("{:?}", composite_alphas));
            //
            let present_mode = {
                use gfx_hal::window::PresentMode::*;
                [Mailbox, Fifo, Relaxed, Immediate]
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
                    .window
                    .get_inner_size()
                    .ok_or("Window doesn't exist!")?
                    .to_physical(window.window.get_hidpi_factor());
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
            info!(logger, "swapchain config"; "swapchain_config" => format!("{:#?}", swapchain_config));
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
            image::load_from_memory(CREATURE_BYTES)
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

        const F32_XY_RGB_UV_QUAD: usize = mem::size_of::<f32>() * (2 + 3 + 2) * 4;
        let vertices =
            BufferBundle::new(&adapter, &device, F32_XY_RGB_UV_QUAD, BufferUsage::VERTEX)?;
        const U16_QUAD_INDICES: usize = mem::size_of::<u16>() * 2 * 3;
        let indexes = BufferBundle::new(&adapter, &device, U16_QUAD_INDICES, BufferUsage::INDEX)?;

        unsafe {
            let mut data_target = device
                .acquire_mapping_writer(&indexes.memory, 0..indexes.requirements.size)
                .map_err(|_| "Failed to require an index buffer mapping writer!")?;
            const INDEX_DATA: &[u16] = &[0, 1, 2, 2, 3, 0];
            data_target[..INDEX_DATA.len()].copy_from_slice(&INDEX_DATA);
            device
                .release_mapping_writer(data_target)
                .map_err(|_| "Couldn't release the index buffer mapping writer!")?;
        }

        Ok(HalState {
            vertices,
            indexes,
            texture,
            descriptor_pool: ManuallyDrop::new(descriptor_pool),
            descriptor_set: ManuallyDrop::new(descriptor_set),
            creation_instant: Instant::now(),
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

    pub fn draw_quad_frame(&mut self, quad: [f32; 4 * (2 + 3 + 2)]) -> Result<(), &'static str> {
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

        unsafe {
            let mut data_target = self
                .device
                .acquire_mapping_writer(
                    self.vertices.memory.deref(),
                    0..self.vertices.requirements.size,
                )
                .map_err(|_| "Failed to acquire a memory writer!")?;
            let points = quad;
            data_target[..points.len()].copy_from_slice(&points);
            self.device
                .release_mapping_writer(data_target)
                .map_err(|_| "Couldn't release the mapping writer")?;
        }

        let duration = Instant::now().duration_since(self.creation_instant);
        let time_f32 = duration.as_secs() as f32 + duration.subsec_nanos() as f32 * 1e-9;
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
                    ShaderStageFlags::FRAGMENT,
                    0,
                    &[time_f32.to_bits()],
                );
                encoder.draw_indexed(0..6, 0, 0..1);
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
        logger: &slog::Logger,
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
            .map_err(|_| "Couldn't compile vertex shader!")?;
        let fragment_compile_artifact = compiler
            .compile_into_spirv(
                FRAGMENT_SOURCE,
                shaderc::ShaderKind::Fragment,
                "fragment.frag",
                "halstate",
                None,
            )
            .map_err(|e| {
                error!(logger, "{}", e);
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
            stride: (mem::size_of::<f32>() * (2 + 3 + 2)) as ElemStride,
            rate: 0,
        }];
        let position_attribute = AttributeDesc {
            location: 0,
            binding: 0,
            element: Element {
                format: Format::Rg32Float,
                offset: 0,
            },
        };
        let color_attribute = AttributeDesc {
            location: 1,
            binding: 0,
            element: Element {
                format: Format::Rgb32Float,
                offset: (mem::size_of::<f32>() * 2) as ElemOffset,
            },
        };
        let uv_attribute = AttributeDesc {
            location: 2,
            binding: 0,
            element: Element {
                format: Format::Rg32Float,
                offset: (mem::size_of::<f32>() * 5) as ElemOffset,
            },
        };
        let attributes: Vec<AttributeDesc> =
            vec![position_attribute, color_attribute, uv_attribute];
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
            let blend_state = BlendState::On {
                color: BlendOp::Add {
                    src: Factor::One,
                    dst: Factor::Zero,
                },
                alpha: BlendOp::Add {
                    src: Factor::One,
                    dst: Factor::Zero,
                },
            };
            BlendDesc {
                logic_op: Some(LogicOp::Copy),
                targets: vec![ColorBlendDesc(ColorMask::ALL, blend_state)],
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
        let bindings = Vec::<DescriptorSetLayoutBinding>::new();
        let immutable_samplers = Vec::<<back::Backend as Backend>::Sampler>::new();
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
                                stage_flags: ShaderStageFlags::FRAGMENT,
                                immutable_samplers: false,
                            },
                            DescriptorSetLayoutBinding {
                                binding: 1,
                                ty: gfx_hal::pso::DescriptorType::Sampler,
                                count: 1,
                                stage_flags: ShaderStageFlags::FRAGMENT,
                                immutable_samplers: false,
                            },
                        ],
                        &[],
                    )
                    .map_err(|_| "Couldn't make a DescriptorSetLayout")?
            }];

        let push_constants = vec![(ShaderStageFlags::FRAGMENT, 0..1)];
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
