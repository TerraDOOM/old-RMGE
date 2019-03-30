use std::{marker::PhantomData, mem::ManuallyDrop};

use gfx_hal::{
    adapter::{Adapter, MemoryTypeId, PhysicalDevice},
    buffer::Usage as BufferUsage,
    device::Device,
    memory::{Properties, Requirements},
    Backend,
};

/// TODO: start using this instead of BufferBundle, this is supposed to be a more Vec like implementation
#[allow(dead_code)]
pub struct GpuBuffer<B: Backend, D: Device<B>, T> {
    buffer: BufferBundle<B, D>,
    cap: usize,
    len: usize,
    _phantom: PhantomData<T>,
}

impl<B: Backend, D: Device<B>, T> GpuBuffer<B, D, T> {
    /// TODO: again, make this work and start using it or something
    #[allow(dead_code)]
    pub fn new(
        adapter: &Adapter<B>,
        device: &D,
        starting_size: usize,
        usage: BufferUsage,
    ) -> Result<Self, &'static str> {
        let buffer = BufferBundle::new(adapter, device, starting_size, usage)?;
        let cap = starting_size;
        let len = 0;
        Ok(GpuBuffer {
            buffer,
            cap,
            len,
            _phantom: PhantomData,
        })
    }
}

pub struct BufferBundle<B: Backend, D: Device<B>> {
    pub buffer: ManuallyDrop<B::Buffer>,
    pub requirements: Requirements,
    pub memory: ManuallyDrop<B::Memory>,
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
