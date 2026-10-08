//! Video and voice transcoding with `ffmpeg` / `ffprobe` (server: `apt install ffmpeg`).
//!
//! Video (posts, stories, topic videos) → HLS VOD with a 360p and a 720p rendition (shorter side; never
//! upscaled), 4-second segments, a master playlist and a poster frame; plus sampled frames for the AI check.
//! Voice notes → AAC (`.m4a`, plays on Android, iOS and the web). The builders are pure and unit-tested; when
//! ffmpeg is missing, video uploads fail with a clear reason and voice notes in common formats are kept as sent.

use crate::config::Config;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;
use tokio::sync::OnceCell;

static AVAILABLE: OnceCell<bool> = OnceCell::const_new();

/// Whether ffmpeg and ffprobe run (checked once).
pub async fn available(cfg: &Config) -> bool {
    let (ff, fp) = (cfg.ffmpeg.clone(), cfg.ffprobe.clone());
    *AVAILABLE
        .get_or_init(|| async move {
            let ok = |bin: String| async move { tokio::process::Command::new(bin).arg("-version").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().await.map(|s| s.success()).unwrap_or(false) };
            ok(ff).await && ok(fp).await
        })
        .await
}

#[derive(Clone, Debug, PartialEq)]
pub struct Probe {
    pub duration_ms: i64,
    /// Display size (rotation applied).
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
    pub has_video: bool,
}

pub fn probe_args(input: &str) -> Vec<String> {
    ["-v", "error", "-print_format", "json", "-show_format", "-show_streams", input].iter().map(|s| s.to_string()).collect()
}

pub fn parse_probe(v: &Value) -> Option<Probe> {
    let streams = v["streams"].as_array()?;
    let video = streams.iter().find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"].as_i64() != Some(1));
    let has_audio = streams.iter().any(|s| s["codec_type"] == "audio");
    let duration = v["format"]["duration"].as_str().and_then(|d| d.parse::<f64>().ok()).or_else(|| video.and_then(|s| s["duration"].as_str()?.parse::<f64>().ok())).unwrap_or(0.0);
    let (mut w, mut h) = video.map(|s| (s["width"].as_u64().unwrap_or(0) as u32, s["height"].as_u64().unwrap_or(0) as u32)).unwrap_or((0, 0));
    let rotation = video
        .and_then(|s| {
            s["side_data_list"].as_array().and_then(|l| l.iter().find_map(|d| d["rotation"].as_f64())).or_else(|| s["tags"]["rotate"].as_str().and_then(|r| r.parse::<f64>().ok()))
        })
        .unwrap_or(0.0);
    if (rotation.abs() as i64) % 180 == 90 {
        std::mem::swap(&mut w, &mut h);
    }
    Some(Probe { duration_ms: (duration * 1000.0).round() as i64, width: w, height: h, has_audio, has_video: video.is_some() })
}

fn even(x: f64) -> u32 {
    let v = x.round() as u32;
    (v + v % 2).max(2)
}

/// Renditions as (name, width, height, video kbit/s): the shorter side scaled to 360 and 720, never upscaled
/// (a source shorter than 720 gets 360p plus its own size when that is at least 480).
pub fn renditions(w: u32, h: u32) -> Vec<(&'static str, u32, u32, u32)> {
    if w == 0 || h == 0 {
        return vec![];
    }
    let short = w.min(h) as f64;
    let scale = |target: f64| -> (u32, u32) {
        let f = (target / short).min(1.0);
        (even(w as f64 * f), even(h as f64 * f))
    };
    let mut out = Vec::new();
    let (w0, h0) = scale(360.0);
    out.push(("360p", w0, h0, 800));
    if short >= 480.0 {
        let (w1, h1) = scale(720.0);
        out.push(("720p", w1, h1, 2800));
    }
    out
}

/// One ffmpeg run: HLS VOD with every rendition, `out_dir/<name>/index.m3u8` + `out_dir/master.m3u8`.
pub fn hls_args(input: &str, out_dir: &str, p: &Probe, max_secs: i64) -> Vec<String> {
    let rs = renditions(p.width, p.height);
    let n = rs.len();
    let mut a: Vec<String> = vec!["-hide_banner".into(), "-y".into(), "-i".into(), input.into(), "-t".into(), max_secs.to_string()];
    let mut filter = if n > 1 { format!("[0:v]split={n}{};", (0..n).map(|i| format!("[v{i}]")).collect::<String>()) } else { String::new() };
    for (i, (_, w, h, _)) in rs.iter().enumerate() {
        let src = if n > 1 { format!("[v{i}]") } else { "[0:v]".into() };
        filter.push_str(&format!("{src}scale={w}:{h},setsar=1[v{i}o];"));
    }
    filter.pop();
    a.push("-filter_complex".into());
    a.push(filter);
    for i in 0..n {
        a.push("-map".into());
        a.push(format!("[v{i}o]"));
        if p.has_audio {
            a.push("-map".into());
            a.push("0:a:0".into());
        }
    }
    for s in ["-c:v", "libx264", "-preset", "veryfast", "-profile:v", "main", "-pix_fmt", "yuv420p", "-g", "48", "-keyint_min", "48", "-sc_threshold", "0"] {
        a.push(s.into());
    }
    for (i, (_, _, _, kbps)) in rs.iter().enumerate() {
        a.push(format!("-b:v:{i}"));
        a.push(format!("{kbps}k"));
        a.push(format!("-maxrate:v:{i}"));
        a.push(format!("{}k", kbps * 107 / 100));
        a.push(format!("-bufsize:v:{i}"));
        a.push(format!("{}k", kbps * 3 / 2));
    }
    if p.has_audio {
        for s in ["-c:a", "aac", "-b:a", "96k", "-ac", "2", "-ar", "44100"] {
            a.push(s.into());
        }
    } else {
        a.push("-an".into());
    }
    for s in ["-f", "hls", "-hls_time", "4", "-hls_playlist_type", "vod", "-hls_flags", "independent_segments", "-master_pl_name", "master.m3u8"] {
        a.push(s.into());
    }
    a.push("-hls_segment_filename".into());
    a.push(format!("{out_dir}/%v/seg_%03d.ts"));
    a.push("-var_stream_map".into());
    a.push(rs.iter().enumerate().map(|(i, (name, ..))| if p.has_audio { format!("v:{i},a:{i},name:{name}") } else { format!("v:{i},name:{name}") }).collect::<Vec<_>>().join(" "));
    a.push(format!("{out_dir}/%v/index.m3u8"));
    a
}

/// Poster frame (JPEG) at `at_ms`.
pub fn poster_args(input: &str, out: &str, at_ms: i64) -> Vec<String> {
    vec!["-hide_banner".into(), "-y".into(), "-ss".into(), format!("{:.3}", at_ms as f64 / 1000.0), "-i".into(), input.into(), "-frames:v".into(), "1".into(), "-q:v".into(), "3".into(), out.into()]
}

/// `n` frames spread over the video (moderation), scaled to 512 px wide.
pub fn frames_args(input: &str, out_pattern: &str, n: u32, duration_ms: i64) -> Vec<String> {
    let secs = (duration_ms.max(1000) as f64) / 1000.0;
    let fps = (n.max(1) as f64) / secs;
    vec![
        "-hide_banner".into(),
        "-y".into(),
        "-i".into(),
        input.into(),
        "-vf".into(),
        format!("fps={fps:.5},scale=512:-2"),
        "-frames:v".into(),
        n.to_string(),
        "-q:v".into(),
        "4".into(),
        out_pattern.into(),
    ]
}

/// Voice note → mono AAC 64 kbit/s.
pub fn voice_args(input: &str, out: &str, max_secs: i64) -> Vec<String> {
    vec!["-hide_banner".into(), "-y".into(), "-i".into(), input.into(), "-t".into(), max_secs.to_string(), "-vn".into(), "-ac".into(), "1".into(), "-c:a".into(), "aac".into(), "-b:a".into(), "64k".into(), "-movflags".into(), "+faststart".into(), out.into()]
}

/// Runs a tool with a timeout; returns stdout.
pub async fn run(bin: &str, args: &[String], timeout: Duration) -> anyhow::Result<Vec<u8>> {
    let child = tokio::process::Command::new(bin).args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true).spawn()?;
    let out = tokio::time::timeout(timeout, child.wait_with_output()).await.map_err(|_| anyhow::anyhow!("{bin} timed out"))??;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("{bin} failed: {}", err.lines().rev().take(3).collect::<Vec<_>>().join(" | "));
    }
    Ok(out.stdout)
}

pub async fn probe(cfg: &Config, input: &Path) -> anyhow::Result<Probe> {
    let out = run(&cfg.ffprobe, &probe_args(&input.to_string_lossy()), Duration::from_secs(30)).await?;
    let v: Value = serde_json::from_slice(&out)?;
    parse_probe(&v).ok_or_else(|| anyhow::anyhow!("ffprobe: no streams"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn probes_and_plans_renditions() {
        let v = json!({"format": {"duration": "12.48"}, "streams": [
            {"codec_type": "video", "width": 1920, "height": 1080, "side_data_list": [{"rotation": -90}]},
            {"codec_type": "audio"}
        ]});
        let p = parse_probe(&v).unwrap();
        assert_eq!(p, Probe { duration_ms: 12480, width: 1080, height: 1920, has_audio: true, has_video: true });
        assert_eq!(renditions(1080, 1920), vec![("360p", 360, 640, 800), ("720p", 720, 1280, 2800)]);
        assert_eq!(renditions(1280, 720), vec![("360p", 640, 360, 800), ("720p", 1280, 720, 2800)]);
        // small sources are not upscaled
        assert_eq!(renditions(320, 240), vec![("360p", 320, 240, 800)]);
        assert!(renditions(0, 0).is_empty());
        let audio_only = parse_probe(&json!({"format": {"duration": "3.0"}, "streams": [{"codec_type": "audio"}]})).unwrap();
        assert!(!audio_only.has_video && audio_only.has_audio);
    }

    #[test]
    fn builds_ffmpeg_commands() {
        let p = Probe { duration_ms: 30_000, width: 1280, height: 720, has_audio: true, has_video: true };
        let a = hls_args("/w/in.mp4", "/w/out", &p, 60).join(" ");
        assert!(a.contains("-i /w/in.mp4 -t 60"));
        assert!(a.contains("-filter_complex [0:v]split=2[v0][v1];[v0]scale=640:360,setsar=1[v0o];[v1]scale=1280:720,setsar=1[v1o]"));
        assert!(a.contains("-map [v0o] -map 0:a:0 -map [v1o] -map 0:a:0"));
        assert!(a.contains("-b:v:0 800k") && a.contains("-b:v:1 2800k"));
        assert!(a.contains("-c:a aac"));
        assert!(a.contains("-hls_time 4 -hls_playlist_type vod"));
        assert!(a.contains("-master_pl_name master.m3u8"));
        assert!(a.contains("-var_stream_map v:0,a:0,name:360p v:1,a:1,name:720p /w/out/%v/index.m3u8"));
        let silent = Probe { has_audio: false, width: 400, height: 300, ..p };
        let s = hls_args("/w/in.mp4", "/w/out", &silent, 600).join(" ");
        assert!(s.contains("-filter_complex [0:v]scale=400:300,setsar=1[v0o]") && s.contains("-an") && !s.contains("0:a:0"));
        assert!(s.contains("-var_stream_map v:0,name:360p"));
        assert_eq!(poster_args("in", "p.jpg", 1500).join(" "), "-hide_banner -y -ss 1.500 -i in -frames:v 1 -q:v 3 p.jpg");
        assert!(frames_args("in", "f_%02d.jpg", 4, 20_000).join(" ").contains("fps=0.20000,scale=512:-2 -frames:v 4"));
        assert!(voice_args("in.webm", "out.m4a", 300).join(" ").contains("-vn -ac 1 -c:a aac -b:a 64k"));
    }
}
