use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

/// Audio buffer pool for reducing memory allocations during recording
pub struct AudioBufferPool {
    pool: Arc<Mutex<VecDeque<Vec<f32>>>>,
    max_size: usize,
    buffer_capacity: usize,
}

impl AudioBufferPool {
    /// Create a new audio buffer pool with specified maximum pool size and buffer capacity
    pub fn new(max_size: usize, buffer_capacity: usize) -> Self {
        Self {
            pool: Arc::new(Mutex::new(VecDeque::with_capacity(max_size))),
            max_size,
            buffer_capacity,
        }
    }

    /// Get a buffer from the pool, or create a new one if pool is empty
    pub fn get_buffer(&self) -> Vec<f32> {
        let mut pool = self.pool.lock().unwrap();

        match pool.pop_front() {
            Some(mut buffer) => {
                buffer.clear();
                buffer.reserve(self.buffer_capacity);
                buffer
            }
            None => {
                // Pool is empty, create a new buffer
                Vec::with_capacity(self.buffer_capacity)
            }
        }
    }

    /// Return a buffer to the pool for reuse
    pub fn return_buffer(&self, mut buffer: Vec<f32>) {
        // Clear the buffer but keep its allocated capacity
        buffer.clear();

        let mut pool = self.pool.lock().unwrap();

        // Only keep buffers if we haven't exceeded max pool size
        if pool.len() < self.max_size {
            pool.push_back(buffer);
        }
        // If pool is full, let the buffer be dropped (deallocated)
    }

    /// Get current pool size (for monitoring)
    pub fn pool_size(&self) -> usize {
        self.pool.lock().unwrap().len()
    }

    /// Clear all buffers in the pool
    pub fn clear(&self) {
        self.pool.lock().unwrap().clear();
    }
}

impl Clone for AudioBufferPool {
    fn clone(&self) -> Self {
        Self {
            pool: Arc::clone(&self.pool),
            max_size: self.max_size,
            buffer_capacity: self.buffer_capacity,
        }
    }
}

/// RAII wrapper that automatically returns buffer to pool when dropped
pub struct PooledBuffer {
    buffer: Option<Vec<f32>>,
    pool: AudioBufferPool,
}

impl PooledBuffer {
    /// Create a new pooled buffer
    pub fn new(pool: AudioBufferPool) -> Self {
        let buffer = pool.get_buffer();
        Self {
            buffer: Some(buffer),
            pool,
        }
    }

    /// Get mutable access to the underlying buffer
    pub fn as_mut(&mut self) -> &mut Vec<f32> {
        self.buffer.as_mut().expect("Buffer should always be available")
    }

    /// Get immutable access to the underlying buffer
    pub fn as_ref(&self) -> &Vec<f32> {
        self.buffer.as_ref().expect("Buffer should always be available")
    }

    /// Consume the wrapper and return the buffer (will not be returned to pool)
    pub fn into_inner(mut self) -> Vec<f32> {
        self.buffer.take().expect("Buffer should always be available")
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            self.pool.return_buffer(buffer);
        }
    }
}

impl std::ops::Deref for PooledBuffer {
    type Target = Vec<f32>;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl std::ops::DerefMut for PooledBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool() {
        let pool = AudioBufferPool::new(3, 1024);
        assert_eq!(pool.pool_size(), 0);

        // Get a buffer and return it
        let buffer = pool.get_buffer();
        assert_eq!(buffer.capacity(), 1024);
        pool.return_buffer(buffer);
        assert_eq!(pool.pool_size(), 1);

        // Get it back
        let buffer2 = pool.get_buffer();
        assert_eq!(pool.pool_size(), 0);
        pool.return_buffer(buffer2);
    }

    #[test]
    fn test_pooled_buffer_raii() {
        let pool = AudioBufferPool::new(2, 512);

        {
            let mut pooled = PooledBuffer::new(pool.clone());
            pooled.push(1.0);
            pooled.push(2.0);
            assert_eq!(pooled.len(), 2);
        } // Buffer should be returned to pool here

        assert_eq!(pool.pool_size(), 1);
    }

    #[test]
    fn test_pool_max_size() {
        let pool = AudioBufferPool::new(2, 256);

        // Fill the pool to capacity
        let buf1 = pool.get_buffer();
        let buf2 = pool.get_buffer();
        let buf3 = pool.get_buffer();

        pool.return_buffer(buf1);
        pool.return_buffer(buf2);
        pool.return_buffer(buf3); // This one should be dropped since pool is full

        assert_eq!(pool.pool_size(), 2);
    }
}