#[cfg(target_os = "windows")]
mod platform {
    use anyhow::{anyhow, Result};
    use windows::{
        core::Interface,
        Win32::{
            Foundation::RPC_E_CHANGED_MODE,
            Media::Audio::{
                eConsole, eRender, Endpoints::IAudioMeterInformation, IAudioSessionControl2,
                IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator,
            },
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
            },
        },
    };

    const AUDIO_ACTIVE_PEAK_THRESHOLD: f32 = 0.0005;

    struct ComGuard {
        should_uninitialize: bool,
    }

    impl ComGuard {
        fn init() -> Result<Self> {
            let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            if result == RPC_E_CHANGED_MODE {
                return Ok(Self {
                    should_uninitialize: false,
                });
            }

            result.ok().map_err(|e| {
                anyhow!("Failed to initialize COM for audio session detection: {e}")
            })?;

            Ok(Self {
                should_uninitialize: true,
            })
        }
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.should_uninitialize {
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }

    pub fn is_any_process_render_audio_active(process_ids: &[u32]) -> Result<bool> {
        if process_ids.is_empty() {
            return Ok(false);
        }

        let _com = ComGuard::init()?;

        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let session_manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
            let session_enumerator = session_manager.GetSessionEnumerator()?;
            let session_count = session_enumerator.GetCount()?;

            for index in 0..session_count {
                let session_control = session_enumerator.GetSession(index)?;
                let session_control2: IAudioSessionControl2 = session_control.cast()?;
                let process_id = session_control2.GetProcessId()?;

                if !process_ids.contains(&process_id) {
                    continue;
                }

                let meter: IAudioMeterInformation = session_control.cast()?;
                let peak = meter.GetPeakValue()?;
                if peak >= AUDIO_ACTIVE_PEAK_THRESHOLD {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use anyhow::Result;

    pub fn is_any_process_render_audio_active(_process_ids: &[u32]) -> Result<bool> {
        Ok(false)
    }
}

pub use platform::is_any_process_render_audio_active;
