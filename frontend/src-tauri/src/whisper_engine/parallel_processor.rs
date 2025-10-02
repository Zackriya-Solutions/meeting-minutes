use std::sync::Arc;
use tokio::sync::{RwLock, mpsc, Semaphore};
use tokio::task::JoinHandle;
use anyhow::{Result, anyhow};
use log::{info, warn, error, debug};
use serde::{Serialize, Deserialize};

use super::whisper_engine::WhisperEngine;
use super::system_monitor::SystemMonitor;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioChunk {
    pub id: u32,
    pub data: Vec<f32>,
    pub sample_rate: u32,
    pub start_time_ms: f64,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub chunk_id: u32,
    pub text: String,
    pub processing_time_ms: u64,
    pub model_used: String,
    pub start_time_ms: f64,
    pub confidence_score: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingError {
    pub chunk_id: u32,
    pub error_message: String,
    pub retry_count: u32,
    pub is_recoverable: bool,
}

#[derive(Debug, Clone)]
pub enum ProcessingEvent {
    ChunkStarted(u32),
    ChunkCompleted(TranscriptionResult),
    ChunkFailed(ProcessingError),
    WorkerStarted(u32),
    WorkerStopped(u32),
    ResourceConstraint(String),
    ProcessingPaused,
    ProcessingResumed,
}

/// Safe parallel processing configuration
#[derive(Debug, Clone)]
pub struct ParallelConfig {
    pub max_workers: usize,          // Never exceed 4 workers (safety limit)
    pub memory_budget_mb: u64,       // Memory budget per worker
    pub max_retries: u32,            // Max retries per chunk (default: 3)
    pub retry_delay_ms: u64,         // Delay between retries
    pub resource_check_interval_ms: u64, // How often to check system resources
    pub enable_fallback_mode: bool,  // Fall back to sequential processing on failures
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            max_workers: 2,              // Conservative default
            memory_budget_mb: 512,       // 512MB per worker
            max_retries: 3,
            retry_delay_ms: 1000,        // 1 second retry delay
            resource_check_interval_ms: 10000, // Check resources every 10 seconds
            enable_fallback_mode: true,  // Always enable fallback for safety
        }
    }
}

/// Worker pool for parallel chunk processing
pub struct ParallelProcessor {
    workers: Vec<Worker>,
    chunk_queue: Arc<RwLock<ChunkQueue>>,
    event_sender: mpsc::UnboundedSender<ProcessingEvent>,
    system_monitor: Arc<SystemMonitor>,
    config: ParallelConfig,
    is_paused: Arc<RwLock<bool>>,
    is_stopped: Arc<RwLock<bool>>,
    semaphore: Arc<Semaphore>, // Limit concurrent workers
}

struct Worker {
    id: u32,
    handle: Option<JoinHandle<Result<()>>>,
    #[allow(dead_code)] // Used in async tasks
    whisper_engine: Arc<RwLock<Option<WhisperEngine>>>,
}

struct ChunkQueue {
    pending: Vec<AudioChunk>,
    processing: std::collections::HashMap<u32, AudioChunk>,
    completed: std::collections::HashMap<u32, TranscriptionResult>,
    failed: std::collections::HashMap<u32, ProcessingError>,
    retry_queue: Vec<(AudioChunk, u32)>, // (chunk, retry_count)
}

impl ParallelProcessor {
    pub fn new(
        config: ParallelConfig,
        system_monitor: Arc<SystemMonitor>,
    ) -> Result<(Self, mpsc::UnboundedReceiver<ProcessingEvent>)> {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        // Safety check: Never exceed 4 workers
        let safe_max_workers = std::cmp::min(config.max_workers, 4);
        if safe_max_workers != config.max_workers {
            warn!("Limiting workers from {} to {} for system safety",
                  config.max_workers, safe_max_workers);
        }

        let mut safe_config = config.clone();
        safe_config.max_workers = safe_max_workers;

        let processor = Self {
            workers: Vec::new(),
            chunk_queue: Arc::new(RwLock::new(ChunkQueue::new())),
            event_sender,
            system_monitor,
            config: safe_config,
            is_paused: Arc::new(RwLock::new(false)),
            is_stopped: Arc::new(RwLock::new(false)),
            semaphore: Arc::new(Semaphore::new(safe_max_workers)),
        };

        info!("Parallel processor initialized with {} workers", safe_max_workers);
        Ok((processor, event_receiver))
    }

    /// Calculate safe worker count based on system resources
    pub async fn calculate_safe_worker_count(&self) -> Result<usize> {
        let worker_count = self.system_monitor.calculate_safe_worker_count().await?;
        let safe_count = std::cmp::min(worker_count, self.config.max_workers);

        info!("Calculated safe worker count: {} (system: {}, config: {})",
              safe_count, worker_count, self.config.max_workers);

        Ok(safe_count)
    }

    /// Start parallel processing with resource-aware worker spawning
    pub async fn start_processing(
        &mut self,
        chunks: Vec<AudioChunk>,
        model_name: String,
    ) -> Result<()> {
        info!("Starting parallel processing of {} chunks with model {}",
              chunks.len(), model_name);

        // Check system resources before starting
        let resource_status = self.system_monitor.check_resource_constraints().await?;
        if !resource_status.can_proceed {
            return Err(anyhow!("Cannot start processing: {}",
                             resource_status.get_primary_constraint()
                             .unwrap_or_else(|| "Resource constraints violated".to_string())));
        }

        // Calculate safe worker count
        let safe_worker_count = self.calculate_safe_worker_count().await?;

        // Initialize chunk queue
        {
            let mut queue = self.chunk_queue.write().await;
            queue.pending = chunks;
            queue.processing.clear();
            queue.completed.clear();
            queue.failed.clear();
            queue.retry_queue.clear();
        }

        // Reset state
        *self.is_paused.write().await = false;
        *self.is_stopped.write().await = false;

        // Spawn workers
        self.spawn_workers(safe_worker_count, model_name).await?;

        // Start resource monitoring task
        self.start_resource_monitoring().await;

        info!("Parallel processing started with {} workers", safe_worker_count);
        Ok(())
    }

    async fn spawn_workers(&mut self, worker_count: usize, model_name: String) -> Result<()> {
        self.workers.clear();

        for worker_id in 0..worker_count {
            let worker = self.create_worker(worker_id as u32, model_name.clone()).await?;
            self.workers.push(worker);
        }

        Ok(())
    }

    async fn create_worker(&self, worker_id: u32, model_name: String) -> Result<Worker> {
        info!("Creating worker {}", worker_id);

        // Create isolated WhisperEngine for this worker
        let whisper_engine = Arc::new(RwLock::new(None));

        // Clone necessary data for worker task
        let chunk_queue = self.chunk_queue.clone();
        let event_sender = self.event_sender.clone();
        let is_paused = self.is_paused.clone();
        let is_stopped = self.is_stopped.clone();
        let semaphore = self.semaphore.clone();
        let config = self.config.clone();
        let engine_ref = whisper_engine.clone();

        // Spawn worker task
        let handle = tokio::spawn(async move {
            // Acquire semaphore permit (limits concurrent workers)
            let _permit = semaphore.acquire().await.map_err(|e| anyhow!("Failed to acquire worker permit: {}", e))?;

            info!("Worker {} started", worker_id);
            let _ = event_sender.send(ProcessingEvent::WorkerStarted(worker_id));

            // Load model for this worker
            {
                let mut engine_guard = engine_ref.write().await;
                let engine = WhisperEngine::new().map_err(|e| anyhow!("Failed to create WhisperEngine: {}", e))?;
                engine.load_model(&model_name).await.map_err(|e| anyhow!("Failed to load model {}: {}", model_name, e))?;
                *engine_guard = Some(engine);
                info!("Worker {} loaded model {}", worker_id, model_name);
            }

            // Main worker loop
            loop {
                // Check if we should stop
                if *is_stopped.read().await {
                    break;
                }

                // Wait if paused
                while *is_paused.read().await && !*is_stopped.read().await {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }

                // Get next chunk to process
                let chunk = {
                    let mut queue = chunk_queue.write().await;

                    // Try retry queue first
                    if let Some((retry_chunk, retry_count)) = queue.retry_queue.pop() {
                        queue.processing.insert(retry_chunk.id, retry_chunk.clone());
                        Some((retry_chunk, retry_count))
                    } else if let Some(chunk) = queue.pending.pop() {
                        queue.processing.insert(chunk.id, chunk.clone());
                        Some((chunk, 0))
                    } else {
                        None
                    }
                };

                match chunk {
                    Some((chunk, retry_count)) => {
                        // Process the chunk
                        let result = Self::process_chunk_safely(
                            &engine_ref,
                            chunk.clone(),
                            &model_name,
                            worker_id
                        ).await;

                        // Handle result
                        let mut queue = chunk_queue.write().await;
                        queue.processing.remove(&chunk.id);

                        match result {
                            Ok(transcription) => {
                                queue.completed.insert(chunk.id, transcription.clone());
                                let _ = event_sender.send(ProcessingEvent::ChunkCompleted(transcription));
                            }
                            Err(e) => {
                                let error = ProcessingError {
                                    chunk_id: chunk.id,
                                    error_message: e.to_string(),
                                    retry_count,
                                    is_recoverable: retry_count < config.max_retries,
                                };

                                if error.is_recoverable {
                                    // Add to retry queue with delay
                                    let chunk_id = chunk.id;
                                    queue.retry_queue.push((chunk, retry_count + 1));
                                    warn!("Worker {} failed chunk {}, queued for retry {}/{}",
                                          worker_id, chunk_id, retry_count + 1, config.max_retries);
                                } else {
                                    // Mark as permanently failed
                                    queue.failed.insert(chunk.id, error.clone());
                                    error!("Worker {} permanently failed chunk {} after {} retries",
                                           worker_id, chunk.id, retry_count);
                                }

                                let _ = event_sender.send(ProcessingEvent::ChunkFailed(error));
                            }
                        }
                    }
                    None => {
                        // No work available, check if we're done
                        let queue = chunk_queue.read().await;
                        if queue.pending.is_empty() && queue.retry_queue.is_empty() && queue.processing.is_empty() {
                            break; // All work completed
                        }

                        // Brief pause before checking again
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    }
                }
            }

            info!("Worker {} stopped", worker_id);
            let _ = event_sender.send(ProcessingEvent::WorkerStopped(worker_id));
            Ok(())
        });

        Ok(Worker {
            id: worker_id,
            handle: Some(handle),
            whisper_engine,
        })
    }

    async fn process_chunk_safely(
        engine_ref: &Arc<RwLock<Option<WhisperEngine>>>,
        chunk: AudioChunk,
        model_name: &str,
        worker_id: u32,
    ) -> Result<TranscriptionResult> {
        let start_time = std::time::Instant::now();

        debug!("Worker {} processing chunk {} ({:.1}s audio)",
               worker_id, chunk.id, chunk.duration_ms / 1000.0);

        let engine_guard = engine_ref.read().await;
        let engine = engine_guard.as_ref()
            .ok_or_else(|| anyhow!("WhisperEngine not loaded for worker {}", worker_id))?;

        // Get language preference
        let language = crate::get_language_preference_internal();

        // Transcribe with timeout to prevent hanging
        let transcription_future = engine.transcribe_audio(chunk.data.clone(), language);
        let timeout_duration = tokio::time::Duration::from_secs(120); // 2 minute timeout per chunk

        let text = tokio::time::timeout(timeout_duration, transcription_future)
            .await
            .map_err(|_| anyhow!("Transcription timeout for chunk {}", chunk.id))?
            .map_err(|e| anyhow!("Transcription failed for chunk {}: {}", chunk.id, e))?;

        let processing_time = start_time.elapsed().as_millis() as u64;

        let result = TranscriptionResult {
            chunk_id: chunk.id,
            text,
            processing_time_ms: processing_time,
            model_used: model_name.to_string(),
            start_time_ms: chunk.start_time_ms,
            confidence_score: None, // TODO: Add confidence scoring if available
        };

        debug!("Worker {} completed chunk {} in {}ms",
               worker_id, chunk.id, processing_time);

        Ok(result)
    }

    async fn start_resource_monitoring(&self) {
        let system_monitor = self.system_monitor.clone();
        let event_sender = self.event_sender.clone();
        let is_stopped = self.is_stopped.clone();
        let is_paused = self.is_paused.clone();
        let check_interval = self.config.resource_check_interval_ms;

        tokio::spawn(async move {
            let mut last_warning = std::time::Instant::now();
            const WARNING_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(30);

            while !*is_stopped.read().await {
                tokio::time::sleep(tokio::time::Duration::from_millis(check_interval)).await;

                match system_monitor.check_resource_constraints().await {
                    Ok(status) => {
                        if !status.can_proceed && last_warning.elapsed() > WARNING_COOLDOWN {
                            let constraint = status.get_primary_constraint()
                                .unwrap_or_else(|| "Resource constraints violated".to_string());

                            warn!("Resource constraint detected: {}", constraint);
                            let _ = event_sender.send(ProcessingEvent::ResourceConstraint(constraint));

                            // Auto-pause processing to prevent system damage
                            *is_paused.write().await = true;
                            let _ = event_sender.send(ProcessingEvent::ProcessingPaused);

                            last_warning = std::time::Instant::now();
                        } else if status.can_proceed && *is_paused.read().await {
                            // Auto-resume if resources are available again
                            info!("Resources available, auto-resuming processing");
                            *is_paused.write().await = false;
                            let _ = event_sender.send(ProcessingEvent::ProcessingResumed);
                        }
                    }
                    Err(e) => {
                        error!("Failed to check system resources: {}", e);
                    }
                }
            }
        });
    }

    pub async fn pause_processing(&self) {
        info!("Pausing parallel processing");
        *self.is_paused.write().await = true;
        let _ = self.event_sender.send(ProcessingEvent::ProcessingPaused);
    }

    pub async fn resume_processing(&self) {
        info!("Resuming parallel processing");
        *self.is_paused.write().await = false;
        let _ = self.event_sender.send(ProcessingEvent::ProcessingResumed);
    }

    pub async fn stop_processing(&mut self) {
        info!("Stopping parallel processing");
        *self.is_stopped.write().await = true;

        // Wait for all workers to complete
        for worker in &mut self.workers {
            if let Some(handle) = worker.handle.take() {
                if let Err(e) = handle.await {
                    error!("Worker {} failed to stop cleanly: {}", worker.id, e);
                }
            }
        }

        self.workers.clear();
        info!("All workers stopped");
    }

    pub async fn get_processing_status(&self) -> ProcessingStatus {
        let queue = self.chunk_queue.read().await;
        ProcessingStatus {
            total_chunks: queue.pending.len() + queue.processing.len() + queue.completed.len() + queue.failed.len(),
            pending_chunks: queue.pending.len(),
            processing_chunks: queue.processing.len(),
            completed_chunks: queue.completed.len(),
            failed_chunks: queue.failed.len(),
            retry_queue_size: queue.retry_queue.len(),
            is_paused: *self.is_paused.read().await,
            is_stopped: *self.is_stopped.read().await,
        }
    }
}

impl ChunkQueue {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
            processing: std::collections::HashMap::new(),
            completed: std::collections::HashMap::new(),
            failed: std::collections::HashMap::new(),
            retry_queue: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingStatus {
    pub total_chunks: usize,
    pub pending_chunks: usize,
    pub processing_chunks: usize,
    pub completed_chunks: usize,
    pub failed_chunks: usize,
    pub retry_queue_size: usize,
    pub is_paused: bool,
    pub is_stopped: bool,
}