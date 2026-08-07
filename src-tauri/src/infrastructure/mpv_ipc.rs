#[cfg(unix)]
use crate::error::AppError;
use crate::error::AppResult;
use crate::infrastructure::remote::{PlaybackReportKind, PlaybackState, RemotePlayback};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::Child;
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const REPORT_INTERVAL: Duration = Duration::from_secs(10);
const CONNECT_ATTEMPTS: usize = 100;

#[cfg(unix)]
type IpcStream = std::os::unix::net::UnixStream;
#[cfg(windows)]
type IpcStream = std::fs::File;

#[derive(Debug)]
pub struct MpvIpcEndpoint {
    address: OsString,
    #[cfg(unix)]
    cleanup_directory: Option<PathBuf>,
}

impl MpvIpcEndpoint {
    pub fn create() -> AppResult<Self> {
        let identifier = Uuid::new_v4().simple().to_string();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;

            // Keep the Unix socket path short enough for macOS and put it in a
            // mode-0700 random directory so no other user can connect to mpv's
            // unauthenticated IPC interface.
            let directory = PathBuf::from("/tmp").join(format!("mpv-enjoy-{identifier}"));
            let mut builder = std::fs::DirBuilder::new();
            builder.mode(0o700);
            builder
                .create(&directory)
                .map_err(|error| AppError::message(format!("无法创建播放器 IPC 目录：{error}")))?;
            let address = directory.join("ipc.sock").into_os_string();
            Ok(Self {
                address,
                cleanup_directory: Some(directory),
            })
        }
        #[cfg(windows)]
        {
            Ok(Self {
                address: OsString::from(format!(r"\\.\pipe\mpv-enjoy-home-{identifier}")),
            })
        }
    }

    pub fn address(&self) -> &OsStr {
        &self.address
    }
}

impl Drop for MpvIpcEndpoint {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(directory) = &self.cleanup_directory {
            let _ = std::fs::remove_file(directory.join("ipc.sock"));
            let _ = std::fs::remove_dir(directory);
        }
    }
}

pub fn monitor_remote_playback(
    mut child: Child,
    endpoint: MpvIpcEndpoint,
    playback: RemotePlayback,
) {
    thread::spawn(move || {
        let Some(mut ipc) = connect(&endpoint, &mut child) else {
            let _ = child.wait();
            return;
        };
        let mut active: Option<TrackedPlayback> = None;
        let mut child_exited = false;

        loop {
            match child.try_wait() {
                Ok(Some(_)) => {
                    child_exited = true;
                    break;
                }
                Ok(None) => {}
                Err(_) => break,
            }

            if let Ok(snapshot) = read_snapshot(&mut ipc)
                && snapshot.playlist_index < playback.items.len()
            {
                update_tracking(&playback, &mut active, snapshot);
            }
            thread::sleep(POLL_INTERVAL);
        }

        if let Some(active) = active {
            report(
                &playback,
                PlaybackReportKind::Stopped,
                active.playlist_index,
                active.state,
            );
        }
        if !child_exited {
            let _ = child.wait();
        }
        drop(endpoint);
    });
}

#[derive(Debug, Clone, Copy)]
struct PlaybackSnapshot {
    playlist_index: usize,
    state: PlaybackState,
}

#[derive(Debug, Clone, Copy)]
struct TrackedPlayback {
    playlist_index: usize,
    state: PlaybackState,
    last_reported_at: Instant,
}

fn update_tracking(
    playback: &RemotePlayback,
    active: &mut Option<TrackedPlayback>,
    snapshot: PlaybackSnapshot,
) {
    let now = Instant::now();
    match active {
        Some(previous) if previous.playlist_index == snapshot.playlist_index => {
            let state_changed = previous.state.paused != snapshot.state.paused
                || previous.state.muted != snapshot.state.muted
                || (previous.state.playback_rate - snapshot.state.playback_rate).abs()
                    > f64::EPSILON;
            previous.state = snapshot.state;
            if state_changed || now.duration_since(previous.last_reported_at) >= REPORT_INTERVAL {
                report(
                    playback,
                    PlaybackReportKind::Progress,
                    previous.playlist_index,
                    previous.state,
                );
                previous.last_reported_at = now;
            }
        }
        Some(previous) => {
            report(
                playback,
                PlaybackReportKind::Stopped,
                previous.playlist_index,
                previous.state,
            );
            report(
                playback,
                PlaybackReportKind::Started,
                snapshot.playlist_index,
                snapshot.state,
            );
            *previous = TrackedPlayback {
                playlist_index: snapshot.playlist_index,
                state: snapshot.state,
                last_reported_at: now,
            };
        }
        None => {
            report(
                playback,
                PlaybackReportKind::Started,
                snapshot.playlist_index,
                snapshot.state,
            );
            *active = Some(TrackedPlayback {
                playlist_index: snapshot.playlist_index,
                state: snapshot.state,
                last_reported_at: now,
            });
        }
    }
}

fn report(
    playback: &RemotePlayback,
    kind: PlaybackReportKind,
    playlist_index: usize,
    state: PlaybackState,
) {
    let Some(reporter) = &playback.reporter else {
        return;
    };
    let Some(item) = playback.items.get(playlist_index) else {
        return;
    };
    if let Err(error) = reporter.report(kind, item, state, playlist_index, playback.items.len()) {
        eprintln!("远程播放进度回传失败：{}", error.0);
    }
}

fn connect(endpoint: &MpvIpcEndpoint, child: &mut Child) -> Option<BufReader<IpcStream>> {
    for _ in 0..CONNECT_ATTEMPTS {
        if child.try_wait().ok().flatten().is_some() {
            return None;
        }
        match open_stream(endpoint.address()) {
            Ok(stream) => return Some(BufReader::new(stream)),
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
    None
}

#[cfg(unix)]
fn open_stream(address: &OsStr) -> io::Result<IpcStream> {
    let stream = IpcStream::connect(PathBuf::from(address))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    Ok(stream)
}

#[cfg(windows)]
fn open_stream(address: &OsStr) -> io::Result<IpcStream> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(PathBuf::from(address))
}

fn read_snapshot(ipc: &mut BufReader<IpcStream>) -> io::Result<PlaybackSnapshot> {
    const PROPERTIES: [&str; 7] = [
        "playlist-pos",
        "time-pos",
        "duration",
        "pause",
        "mute",
        "volume",
        "speed",
    ];
    for (index, property) in PROPERTIES.iter().enumerate() {
        serde_json::to_writer(
            &mut *ipc.get_mut(),
            &json!({
                "command": ["get_property", property],
                "request_id": index + 1,
            }),
        )
        .map_err(io::Error::other)?;
        ipc.get_mut().write_all(b"\n")?;
    }
    ipc.get_mut().flush()?;

    let mut responses = HashMap::new();
    for _ in 0..64 {
        let mut line = String::new();
        if ipc.read_line(&mut line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "mpv IPC 已关闭",
            ));
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(request_id) = value.get("request_id").and_then(Value::as_u64) else {
            continue;
        };
        if (1..=PROPERTIES.len() as u64).contains(&request_id) {
            responses.insert(
                request_id,
                value.get("data").cloned().unwrap_or(Value::Null),
            );
        }
        if responses.len() == PROPERTIES.len() {
            break;
        }
    }

    let playlist_index = responses
        .get(&1)
        .and_then(Value::as_i64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| io::Error::other("mpv 尚未载入播放条目"))?;
    let position_seconds = finite_number(responses.get(&2))
        .ok_or_else(|| io::Error::other("mpv 尚未返回播放位置"))?
        .max(0.0);
    Ok(PlaybackSnapshot {
        playlist_index,
        state: PlaybackState {
            position_seconds,
            duration_seconds: finite_number(responses.get(&3)).filter(|value| *value > 0.0),
            paused: responses.get(&4).and_then(Value::as_bool).unwrap_or(false),
            muted: responses.get(&5).and_then(Value::as_bool).unwrap_or(false),
            volume: finite_number(responses.get(&6)),
            playback_rate: finite_number(responses.get(&7)).unwrap_or(1.0),
        },
    })
}

fn finite_number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_endpoint_has_a_short_unique_address() {
        let first = MpvIpcEndpoint::create().unwrap();
        let second = MpvIpcEndpoint::create().unwrap();
        assert_ne!(first.address(), second.address());
        assert!(first.address().to_string_lossy().len() < 100);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let directory = first.cleanup_directory.as_ref().unwrap();
            let mode = std::fs::metadata(directory).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700);
        }
    }

    #[cfg(unix)]
    #[test]
    fn reads_playback_state_from_json_ipc_responses() {
        use std::os::unix::net::UnixStream;

        let (client, mut server) = UnixStream::pair().unwrap();
        let responder = thread::spawn(move || {
            let mut reader = BufReader::new(server.try_clone().unwrap());
            for _ in 0..7 {
                let mut request = String::new();
                reader.read_line(&mut request).unwrap();
            }
            server.write_all(b"{\"event\":\"file-loaded\"}\n").unwrap();
            for (request_id, data) in [
                (1, json!(2)),
                (2, json!(123.5)),
                (3, json!(1400.0)),
                (4, json!(true)),
                (5, json!(false)),
                (6, json!(72.0)),
                (7, json!(1.25)),
            ] {
                serde_json::to_writer(
                    &mut server,
                    &json!({
                        "data": data,
                        "error": "success",
                        "request_id": request_id,
                    }),
                )
                .unwrap();
                server.write_all(b"\n").unwrap();
            }
        });

        let snapshot = read_snapshot(&mut BufReader::new(client)).unwrap();
        assert_eq!(snapshot.playlist_index, 2);
        assert_eq!(snapshot.state.position_seconds, 123.5);
        assert_eq!(snapshot.state.duration_seconds, Some(1400.0));
        assert!(snapshot.state.paused);
        assert!(!snapshot.state.muted);
        assert_eq!(snapshot.state.volume, Some(72.0));
        assert_eq!(snapshot.state.playback_rate, 1.25);
        responder.join().unwrap();
    }
}
