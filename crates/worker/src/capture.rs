use crate::{pipeline::SpeechPipeline, status, Queue};
use linguist_core::audio::FRAME;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use wasapi::{AudioClient, Direction, SampleType, StreamMode, WaveFormat};
use windows::Win32::{
    Foundation::CloseHandle,
    System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    },
};

fn cs2_pid() -> Result<Option<u32>, String> {
    unsafe {
        let snapshot =
            CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| e.to_string())?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = None;
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let length = entry
                    .szExeFile
                    .iter()
                    .position(|c| *c == 0)
                    .unwrap_or(entry.szExeFile.len());
                if String::from_utf16_lossy(&entry.szExeFile[..length])
                    .eq_ignore_ascii_case("cs2.exe")
                {
                    found = Some(entry.th32ProcessID);
                    break;
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        Ok(found)
    }
}

pub fn watch_cs2(vad: &str, queue: Queue, stream: Arc<AtomicU64>) -> Result<(), String> {
    wasapi::initialize_mta().ok().map_err(|e| e.to_string())?;
    loop {
        status("waiting", "Waiting for CS2");
        let pid = loop {
            if let Some(pid) = cs2_pid()? {
                break pid;
            }
            std::thread::sleep(Duration::from_secs(1));
        };
        let epoch = stream.fetch_add(1, Ordering::SeqCst) + 1;
        let result = capture(pid, vad, queue.clone(), &stream, epoch);
        stream.fetch_add(1, Ordering::SeqCst); // In-flight results from the old game/device are invalid.
        queue.0.lock().unwrap().clear();
        if let Err(error) = result {
            status(
                "reconnecting",
                format!("Audio disconnected: {error}. Retrying…"),
            );
            std::thread::sleep(Duration::from_secs(2));
        }
    }
}

fn capture(
    pid: u32,
    vad: &str,
    queue: Queue,
    stream: &AtomicU64,
    epoch: u64,
) -> Result<(), String> {
    let mut pipeline = SpeechPipeline::new(vad, queue, epoch)?;
    let mut client =
        AudioClient::new_application_loopback_client(pid, true).map_err(|e| e.to_string())?;
    // Let the Windows audio engine downmix and resample; do not approximate resampling.
    let format = WaveFormat::new(32, 32, &SampleType::Float, 16000, 1, None);
    client
        .initialize_client(
            &format,
            &Direction::Capture,
            &StreamMode::EventsShared {
                autoconvert: true,
                buffer_duration_hns: 200_000,
            },
        )
        .map_err(|e| e.to_string())?;
    let event = client.set_get_eventhandle().map_err(|e| e.to_string())?;
    let capture = client.get_audiocaptureclient().map_err(|e| e.to_string())?;
    client.start_stream().map_err(|e| e.to_string())?;
    status("listening", "Listening to CS2");
    let mut bytes = VecDeque::new();
    let mut last_pid_check = Instant::now();
    let mut last_packet = Instant::now();
    let mut seen_packet = false;
    loop {
        if last_pid_check.elapsed() >= Duration::from_secs(1) {
            if cs2_pid()? != Some(pid) {
                break;
            }
            last_pid_check = Instant::now();
        }
        if stream.load(Ordering::SeqCst) != epoch {
            break;
        }
        let _ = event.wait_for_event(100); // A quiet process may not signal; this is not a device error.
        while capture
            .get_next_packet_size()
            .map_err(|e| e.to_string())?
            .unwrap_or(0)
            > 0
        {
            let info = capture
                .read_from_device_to_deque(&mut bytes)
                .map_err(|e| e.to_string())?;
            if info.flags.data_discontinuity && seen_packet {
                return Err("Audio discontinuity".into());
            }
            seen_packet = true;
            last_packet = Instant::now();
        }
        if bytes.len() > 16000 * 4 * 2 {
            return Err("Capture buffer overrun".into());
        }
        while bytes.len() >= FRAME * 4 {
            let mut frame = [0.0; FRAME];
            for sample in &mut frame {
                let raw = [
                    bytes.pop_front().unwrap(),
                    bytes.pop_front().unwrap(),
                    bytes.pop_front().unwrap(),
                    bytes.pop_front().unwrap(),
                ];
                let value = f32::from_le_bytes(raw);
                *sample = if value.is_finite() {
                    value.clamp(-1.0, 1.0)
                } else {
                    0.0
                };
            }
            let delay = Duration::from_secs_f64(bytes.len() as f64 / (16000.0 * 4.0));
            pipeline.frame(&frame, Instant::now() - delay)?;
        }
        if last_packet.elapsed() > Duration::from_millis(100) {
            // Flush the last utterance even when Windows stops issuing silent packets.
            for _ in 0..4 {
                pipeline.frame(&[0.0; FRAME], Instant::now())?;
            }
            last_packet = Instant::now();
        }
    }
    client.stop_stream().map_err(|e| e.to_string())?;
    Ok(())
}
