//! Floating panel host and local HTTP/JSON IPC bridge for Marbble instruments, sampler, and effects.
//!
//! Panel specs, parameter schemas, and WebView UI bridge modeled on Marbble
//! (<https://github.com/JackMechem/marbble>), Copyright (c) 2026 Jack Mechem, MIT License.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use egui::{Color32, RichText, Ui};
use prowave_engine::marbble_dsp::fx::{self, EffectInfo, ParamKind, ParamSpec, Unit, MAX_METERS};
use prowave_engine::marbble_dsp::instrument::{self, InstrumentInfo};

/// Shared live state exposed over the local HTTP/JSON bridge for WebView/React panels.
#[derive(Debug, Clone)]
pub struct PanelBridgeState {
    pub plugin_id: String,
    pub plugin_name: String,
    pub category: String,
    pub params: HashMap<u32, f32>,
    pub meters: [f32; MAX_METERS],
    pub pending_edits: Vec<(u32, f32)>,
    pub pending_preset: Option<String>,
}

impl PanelBridgeState {
    pub fn from_effect(info: &EffectInfo) -> Self {
        let mut params = HashMap::new();
        for p in info.params {
            params.insert(p.id, p.default);
        }
        Self {
            plugin_id: info.id.to_string(),
            plugin_name: info.name.to_string(),
            category: info.category.to_string(),
            params,
            meters: [0.0; MAX_METERS],
            pending_edits: Vec::new(),
            pending_preset: None,
        }
    }

    pub fn from_instrument(
        info: &InstrumentInfo,
        current: &std::collections::BTreeMap<u32, f32>,
    ) -> Self {
        let mut params = HashMap::new();
        for p in info.params {
            params.insert(p.id, current.get(&p.id).copied().unwrap_or(p.default));
        }
        Self {
            plugin_id: info.id.to_string(),
            plugin_name: info.name.to_string(),
            category: info.group.to_string(),
            params,
            meters: [0.0; MAX_METERS],
            pending_edits: Vec::new(),
            pending_preset: None,
        }
    }
}

/// Lightweight local HTTP+JSON bridge so floating WebView/browser panels (e.g. Marbble React UI
/// or `wry` when enabled) can read specs/meters and send parameter/preset updates to `prowave-engine`.
pub struct MarbblePanelBridge {
    addr: SocketAddr,
    state: Arc<Mutex<PanelBridgeState>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

impl MarbblePanelBridge {
    pub fn start(initial: PanelBridgeState) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let state = Arc::new(Mutex::new(initial));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let st = state.clone();
        let flag = stop.clone();
        thread::Builder::new()
            .name("marbble-panel-bridge".into())
            .spawn(move || {
                while !flag.load(std::sync::atomic::Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let _ = handle_client(stream, &st);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(std::time::Duration::from_millis(8));
                        }
                        Err(_) => break,
                    }
                }
            })?;

        Ok(Self { addr, state, stop })
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn drain_edits(&self) -> (Vec<(u32, f32)>, Option<String>) {
        let Ok(mut guard) = self.state.lock() else {
            return (Vec::new(), None);
        };
        let edits = std::mem::take(&mut guard.pending_edits);
        let preset = guard.pending_preset.take();
        (edits, preset)
    }

    pub fn update_meters(&self, meters: &[f32; MAX_METERS]) {
        if let Ok(mut guard) = self.state.lock() {
            guard.meters = *meters;
        }
    }
}

impl Drop for MarbblePanelBridge {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn handle_client(
    mut stream: TcpStream,
    state: &Arc<Mutex<PanelBridgeState>>,
) -> std::io::Result<()> {
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Ok(());
    }
    let req = String::from_utf8_lossy(&buf[..n]);
    let first_line = req.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let path = parts.next().unwrap_or("/");

    if method == "GET" && path == "/api/state" {
        let body = state_json(state);
        return write_http(&mut stream, "200 OK", "application/json", &body);
    }

    if method == "POST" && path == "/api/param" {
        if let Some((_, body)) = req.split_once("\r\n\r\n") {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(body.trim()) {
                if let (Some(id), Some(v)) = (val["id"].as_u64(), val["value"].as_f64()) {
                    if let Ok(mut guard) = state.lock() {
                        guard.params.insert(id as u32, v as f32);
                        guard.pending_edits.push((id as u32, v as f32));
                    }
                }
            }
        }
        return write_http(&mut stream, "200 OK", "application/json", r#"{"ok":true}"#);
    }

    if method == "POST" && path == "/api/preset" {
        if let Some((_, body)) = req.split_once("\r\n\r\n") {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(body.trim()) {
                if let Some(name) = val["name"].as_str() {
                    if let Ok(mut guard) = state.lock() {
                        guard.pending_preset = Some(name.to_string());
                    }
                }
            }
        }
        return write_http(&mut stream, "200 OK", "application/json", r#"{"ok":true}"#);
    }

    let html = panel_html(state);
    write_http(&mut stream, "200 OK", "text/html; charset=utf-8", &html)
}

fn write_http(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body.as_bytes())
}

fn param_spec_json(p: &ParamSpec, val: f32) -> serde_json::Value {
    let (kind_str, choices): (&str, Vec<&str>) = match p.kind {
        ParamKind::Continuous => ("continuous", Vec::new()),
        ParamKind::Toggle => ("toggle", Vec::new()),
        ParamKind::Choice(labels) => ("choice", labels.to_vec()),
    };
    serde_json::json!({
        "id": p.id,
        "name": p.name,
        "min": p.min,
        "max": p.max,
        "default": p.default,
        "value": val,
        "log": p.log,
        "unit": format!("{:?}", p.unit),
        "kind": kind_str,
        "choices": choices,
    })
}

fn state_json(state: &Arc<Mutex<PanelBridgeState>>) -> String {
    let Ok(guard) = state.lock() else {
        return "{}".to_string();
    };
    if let Some(inf) = fx::find(&guard.plugin_id) {
        let specs: Vec<serde_json::Value> = inf
            .params
            .iter()
            .map(|p| param_spec_json(p, guard.params.get(&p.id).copied().unwrap_or(p.default)))
            .collect();
        let presets: Vec<&str> = fx::factory_presets(&guard.plugin_id)
            .iter()
            .map(|pr| pr.name)
            .collect();
        return serde_json::json!({
            "id": guard.plugin_id,
            "name": guard.plugin_name,
            "category": guard.category,
            "params": specs,
            "sections": [],
            "presets": presets,
            "meters": guard.meters,
        })
        .to_string();
    }
    if let Some(inf) = instrument::find(&guard.plugin_id) {
        let specs: Vec<serde_json::Value> = inf
            .params
            .iter()
            .map(|p| param_spec_json(p, guard.params.get(&p.id).copied().unwrap_or(p.default)))
            .collect();
        let sections: Vec<serde_json::Value> = inf
            .sections
            .iter()
            .map(|s| serde_json::json!({ "title": s.title, "params": s.params }))
            .collect();
        let presets: Vec<&str> = inf.presets.iter().map(|pr| pr.name).collect();
        return serde_json::json!({
            "id": guard.plugin_id,
            "name": guard.plugin_name,
            "category": guard.category,
            "params": specs,
            "sections": sections,
            "presets": presets,
            "meters": guard.meters,
        })
        .to_string();
    }
    "{}".to_string()
}

fn panel_html(state: &Arc<Mutex<PanelBridgeState>>) -> String {
    let name = state
        .lock()
        .map(|g| g.plugin_name.clone())
        .unwrap_or_else(|_| "Marbble Panel".to_string());
    format!(
        r#"<!doctype html>
<html>
<head><meta charset="utf-8"><title>{name} - Marbble Panel</title>
<style>
*{{box-sizing:border-box}}
body{{margin:0;background:#18181a;color:#e6e6eb;font:12px/1.4 system-ui,sans-serif;padding:14px}}
.hdr{{display:flex;align-items:center;gap:10px;margin-bottom:10px}}
.hdr h2{{margin:0;font-size:15px;color:#fff}}
.badge{{font-size:11px;color:#4cc9f0;background:#222327;border:1px solid #303136;border-radius:4px;padding:2px 7px}}
.presets{{display:flex;flex-wrap:wrap;gap:6px;margin-bottom:12px}}
.presets button{{background:#232327;color:#d6d8de;border:1px solid #34353a;border-radius:4px;padding:4px 9px;cursor:pointer;font-size:11px}}
.presets button:hover{{background:#2b6cb0;color:#fff;border-color:#2b6cb0}}
.sections{{display:grid;grid-template-columns:repeat(auto-fill,minmax(210px,1fr));gap:10px}}
.sec{{background:#202023;border:1px solid #2e2e32;border-radius:6px;padding:10px}}
.sec h3{{margin:0 0 8px;font-size:11px;text-transform:uppercase;letter-spacing:.05em;color:#4cc9f0}}
.row{{margin-bottom:8px}}
.lbl{{display:flex;justify-content:space-between;color:#a0a3ac;margin-bottom:3px;font-size:11px}}
input[type=range]{{width:100%;accent-color:#4cc9f0}}
select,button.tgl{{width:100%;background:#18181a;color:#e6e6eb;border:1px solid #34353a;border-radius:4px;padding:4px 6px;font-size:11px}}
button.tgl.on{{background:#2b6cb0;border-color:#2b6cb0;color:#fff}}
</style></head>
<body>
<div class="hdr"><h2>{name}</h2><span id="cat" class="badge"></span></div>
<div id="presets" class="presets"></div>
<div id="sections" class="sections"></div>
<script>
async function post(url, body){{await fetch(url,{{method:'POST',body:JSON.stringify(body)}});}}
function renderParam(p){{
  const d=document.createElement('div');d.className='row';
  if(p.kind==='toggle'){{
    const b=document.createElement('button');b.className='tgl'+(p.value>=0.5?' on':'');
    b.textContent=p.name+': '+(p.value>=0.5?'On':'Off');
    b.onclick=()=>{{p.value=p.value>=0.5?0:1;b.className='tgl'+(p.value>=0.5?' on':'');b.textContent=p.name+': '+(p.value>=0.5?'On':'Off');post('/api/param',{{id:p.id,value:p.value}});}};
    d.appendChild(b);return d;
  }}
  if(p.kind==='choice'&&p.choices&&p.choices.length){{
    d.innerHTML=`<div class="lbl"><span>${{p.name}}</span></div>`;
    const s=document.createElement('select');
    p.choices.forEach((c,i)=>{{const o=document.createElement('option');o.value=i;o.textContent=c;if(Math.round(p.value)===i)o.selected=true;s.appendChild(o);}});
    s.onchange=()=>post('/api/param',{{id:p.id,value:Number(s.value)}});
    d.appendChild(s);return d;
  }}
  d.innerHTML=`<div class="lbl"><span>${{p.name}}</span><span id="v${{p.id}}">${{p.value.toFixed(2)}}</span></div>
    <input type="range" min="${{p.min}}" max="${{p.max}}" step="${{(p.max-p.min)/200}}" value="${{p.value}}">`;
  const inp=d.querySelector('input');
  inp.oninput=()=>{{document.getElementById('v'+p.id).textContent=Number(inp.value).toFixed(2);post('/api/param',{{id:p.id,value:Number(inp.value)}});}};
  return d;
}}
async function load(){{
  const r=await fetch('/api/state');const s=await r.json();
  document.getElementById('cat').textContent=s.category||'';
  const pb=document.getElementById('presets');pb.innerHTML='';
  (s.presets||[]).forEach(n=>{{const b=document.createElement('button');b.textContent=n;b.onclick=async()=>{{await post('/api/preset',{{name:n}});load();}};pb.appendChild(b);}});
  const byId=new Map((s.params||[]).map(p=>[p.id,p]));
  const sc=document.getElementById('sections');sc.innerHTML='';
  const secs=(s.sections&&s.sections.length)?s.sections:[{{title:'Parameters',params:(s.params||[]).map(p=>p.id)}}];
  for(const sec of secs){{
    const box=document.createElement('div');box.className='sec';
    box.innerHTML=`<h3>${{sec.title}}</h3>`;
    for(const id of sec.params){{const p=byId.get(id);if(p)box.appendChild(renderParam(p));}}
    sc.appendChild(box);
  }}
}}
load();
</script></body></html>"#
    )
}

/// Renders any Marbble `EffectInfo` panel cleanly inside an `egui` floating or docked window.
pub fn show_effect_panel(
    ui: &mut Ui,
    info: &EffectInfo,
    values: &mut HashMap<u32, f32>,
    meters: &[f32; MAX_METERS],
    on_param: &mut dyn FnMut(u32, f32),
    on_preset: &mut dyn FnMut(&'static [(&'static str, &[(u32, f32)])], usize),
) {
    let _ = on_preset;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(info.name)
                .strong()
                .size(14.0)
                .color(Color32::WHITE),
        );
        ui.label(
            RichText::new(info.category)
                .size(11.0)
                .color(Color32::from_rgb(0x8e, 0x94, 0xa0)),
        );
    });
    let presets = fx::factory_presets(info.id);
    if !presets.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for pr in presets {
                if ui.small_button(pr.name).clicked() {
                    for &(id, val) in pr.values {
                        values.insert(id, val);
                        on_param(id, val);
                    }
                }
            }
        });
    }
    if !info.meters.is_empty() {
        ui.horizontal(|ui| {
            for (idx, &m_name) in info.meters.iter().enumerate() {
                if let Some(&val) = meters.get(idx) {
                    ui.label(
                        RichText::new(format!("{m_name}: {val:.1}"))
                            .size(11.0)
                            .color(Color32::from_rgb(0x4c, 0xc9, 0xf0)),
                    );
                }
            }
        });
    }
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for p in info.params {
            draw_param_row(ui, p, values, on_param);
        }
    });
}

/// Floating `egui::Window` for the active track's Marbble Synth or Sampler (`UiState::show_marbble_inst`).

/// Renders any Marbble `InstrumentInfo` panel grouped by its `Section`s with factory presets.
#[allow(dead_code)]
pub fn show_instrument_panel(
    ui: &mut Ui,
    info: &InstrumentInfo,
    values: &mut HashMap<u32, f32>,
    meters: &[f32; MAX_METERS],
    on_param: &mut dyn FnMut(u32, f32),
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(info.name)
                .strong()
                .size(14.0)
                .color(Color32::WHITE),
        );
        ui.label(
            RichText::new(info.group)
                .size(11.0)
                .color(Color32::from_rgb(0x8e, 0x94, 0xa0)),
        );
    });
    if !info.presets.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for pr in info.presets {
                if ui.small_button(pr.name).clicked() {
                    for p in info.params {
                        values.insert(p.id, p.default);
                        on_param(p.id, p.default);
                    }
                    for &(id, val) in pr.values {
                        values.insert(id, val);
                        on_param(id, val);
                    }
                }
            }
        });
    }
    if !info.meters.is_empty() {
        ui.horizontal(|ui| {
            for (idx, &m_name) in info.meters.iter().enumerate() {
                if let Some(&val) = meters.get(idx) {
                    ui.label(
                        RichText::new(format!("{m_name}: {val:.1}"))
                            .size(11.0)
                            .color(Color32::from_rgb(0x4c, 0xc9, 0xf0)),
                    );
                }
            }
        });
    }
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for sec in info.sections {
            ui.label(
                RichText::new(sec.title)
                    .strong()
                    .size(12.0)
                    .color(Color32::from_rgb(0x4c, 0xc9, 0xf0)),
            );
            for &id in sec.params {
                if let Some(p) = info.param(id) {
                    draw_param_row(ui, p, values, on_param);
                }
            }
            ui.add_space(6.0);
        }
    });
}

/// Floating `egui::Window` for the active track's Marbble Synth or Sampler (`UiState::show_marbble_inst`).

fn draw_param_row(
    ui: &mut Ui,
    p: &ParamSpec,
    values: &mut HashMap<u32, f32>,
    on_param: &mut dyn FnMut(u32, f32),
) {
    let mut cur = values.get(&p.id).copied().unwrap_or(p.default);
    ui.horizontal(|ui| {
        ui.set_min_width(240.0);
        ui.label(
            RichText::new(p.name)
                .size(11.5)
                .color(Color32::from_rgb(0xd8, 0xdc, 0xe4)),
        );
        let changed = match p.kind {
            ParamKind::Toggle => {
                let mut b = cur >= 0.5;
                let r = ui.checkbox(&mut b, "");
                if r.changed() {
                    cur = if b { 1.0 } else { 0.0 };
                }
                r.changed()
            }
            ParamKind::Choice(labels) => {
                let idx = (cur.round() as usize).min(labels.len().saturating_sub(1));
                let mut chosen = idx;
                egui::ComboBox::from_id_salt(("marbble-param", p.id))
                    .selected_text(labels.get(idx).copied().unwrap_or(""))
                    .show_ui(ui, |ui| {
                        for (i, &lbl) in labels.iter().enumerate() {
                            ui.selectable_value(&mut chosen, i, lbl);
                        }
                    });
                if chosen != idx {
                    cur = chosen as f32;
                    true
                } else {
                    false
                }
            }
            ParamKind::Continuous => {
                let slider = egui::Slider::new(&mut cur, p.min..=p.max)
                    .logarithmic(p.log && p.min > 0.0)
                    .suffix(unit_suffix(p.unit));
                ui.add(slider).changed()
            }
        };
        if changed {
            values.insert(p.id, cur);
            on_param(p.id, cur);
        }
    });
}

/// Floating `egui::Window` for the active track's Marbble Synth or Sampler (`UiState::show_marbble_inst`).

fn unit_suffix(u: Unit) -> &'static str {
    match u {
        Unit::Db => " dB",
        Unit::Ms => " ms",
        Unit::Hz => " Hz",
        Unit::Ratio => ":1",
        Unit::Percent => " %",
        Unit::Semitones => " st",
        Unit::Degrees => " deg",
        Unit::None | Unit::Note => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_serves_state_and_accepts_param_edits() {
        let info = fx::find("builtin:compressor").expect("compressor info");
        let bridge =
            MarbblePanelBridge::start(PanelBridgeState::from_effect(&info)).expect("start bridge");
        let addr = bridge.addr;

        let mut get_stream = TcpStream::connect(addr).expect("connect GET");
        get_stream
            .write_all(b"GET /api/state HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .expect("write GET");
        let mut resp = String::new();
        get_stream.read_to_string(&mut resp).expect("read GET");
        assert!(resp.contains("builtin:compressor"));

        let body = r#"{"id":0,"value":-18.0}"#;
        let post = format!(
            "POST /api/param HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let mut post_stream = TcpStream::connect(addr).expect("connect POST");
        post_stream.write_all(post.as_bytes()).expect("write POST");
        let mut post_resp = String::new();
        post_stream
            .read_to_string(&mut post_resp)
            .expect("read POST");
        assert!(post_resp.contains(r#"{"ok":true}"#));

        let (edits, _) = bridge.drain_edits();
        assert_eq!(edits, vec![(0, -18.0)]);
    }
}

struct WebViewSession {
    track_idx: usize,
    plugin_id: String,
    bridge: MarbblePanelBridge,
    child: Option<Child>,
}

fn webview_store() -> &'static Mutex<Option<WebViewSession>> {
    static STORE: std::sync::OnceLock<Mutex<Option<WebViewSession>>> = std::sync::OnceLock::new();
    STORE.get_or_init(|| Mutex::new(None))
}

fn spawn_webkit_window(url: &str, title: &str) -> Option<Child> {
    let py = r#"
import sys, gi
gi.require_version('Gtk', '3.0')
gi.require_version('WebKit2', '4.1')
from gi.repository import Gtk, WebKit2
win = Gtk.Window(title=sys.argv[2])
win.set_default_size(760, 560)
wv = WebKit2.WebView()
wv.load_uri(sys.argv[1])
win.add(wv)
win.connect('destroy', Gtk.main_quit)
win.show_all()
Gtk.main()
"#;
    Command::new("python3")
        .arg("-c")
        .arg(py)
        .arg(url)
        .arg(title)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

pub fn show_track_instrument_window(
    ctx: &egui::Context,
    p: &mut crate::model::Project,
    s: &mut crate::UiState,
) {
    let Some(ti) = s.show_marbble_inst else {
        return;
    };
    let Some(track) = p.tracks.get_mut(ti) else {
        s.show_marbble_inst = None;
        return;
    };
    let Some(use_) = track.marbble_inst.as_mut() else {
        s.show_marbble_inst = None;
        return;
    };
    let Some(info) = prowave_engine::marbble_dsp::instrument::find(&use_.id) else {
        s.show_marbble_inst = None;
        return;
    };

    // Drain any edits made inside the WebView preview window into the track's instrument state.
    if let Ok(mut guard) = webview_store().lock() {
        if let Some(sess) = guard.as_mut() {
            if sess.track_idx == ti && sess.plugin_id == use_.id {
                let (edits, preset) = sess.bridge.drain_edits();
                for (id, v) in edits {
                    use_.params.insert(id, v);
                }
                if let Some(preset_name) = preset {
                    if let Some(pr) = info.presets.iter().find(|pr| pr.name == preset_name) {
                        for p in info.params {
                            use_.params.insert(p.id, p.default);
                        }
                        for &(id, v) in pr.values {
                            use_.params.insert(id, v);
                        }
                    }
                }
            }
        }
    }

    let mut open = true;
    let mut vals: HashMap<u32, f32> = use_.params.iter().map(|(&k, &v)| (k, v)).collect();
    let meters = [0.0; MAX_METERS];
    egui::Window::new(format!("{} - {}", track.name, info.name))
        .open(&mut open)
        .default_size([460.0, 440.0])
        .resizable(true)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open WebView Preview").clicked() {
                    if let Ok(mut guard) = webview_store().lock() {
                        if let Some(mut old) = guard.take() {
                            if let Some(mut ch) = old.child.take() {
                                let _ = ch.kill();
                            }
                        }
                        if let Ok(bridge) = MarbblePanelBridge::start(
                            PanelBridgeState::from_instrument(&info, &use_.params),
                        ) {
                            let title = format!("{} - {} (Marbble WebView)", track.name, info.name);
                            let child = spawn_webkit_window(&bridge.url(), &title);
                            *guard = Some(WebViewSession {
                                track_idx: ti,
                                plugin_id: use_.id.clone(),
                                bridge,
                                child,
                            });
                        }
                    }
                }
            });
            ui.separator();
            show_instrument_panel(ui, &info, &mut vals, &meters, &mut |id, v| {
                use_.params.insert(id, v);
            });
        });
    if !open {
        s.show_marbble_inst = None;
    }
}

/// Opens (or focuses) the floating `WebKit2` WebView panel for a Marbble effect (`"builtin:*"`).
pub fn open_effect_webview(marbble_id: &str, title: &str, current_params: &[f32]) {
    let Some(info) = fx::find(marbble_id) else {
        return;
    };
    let mut state = PanelBridgeState::from_effect(&info);
    for (idx, spec) in info.params.iter().enumerate() {
        if let Some(&v) = current_params.get(idx) {
            state.params.insert(spec.id, v);
        }
    }
    if let Ok(mut guard) = webview_store().lock() {
        if let Some(mut old) = guard.take() {
            if let Some(mut ch) = old.child.take() {
                let _ = ch.kill();
            }
        }
        if let Ok(bridge) = MarbblePanelBridge::start(state) {
            let child = spawn_webkit_window(&bridge.url(), title);
            *guard = Some(WebViewSession {
                track_idx: usize::MAX,
                plugin_id: marbble_id.to_string(),
                bridge,
                child,
            });
        }
    }
}

/// Drains parameter & preset edits from an active Marbble effect WebView into `Insert::params`.
pub fn drain_effect_webview(marbble_id: &str, params: &mut [f32]) {
    let Some(info) = fx::find(marbble_id) else {
        return;
    };
    let Ok(mut guard) = webview_store().lock() else {
        return;
    };
    let Some(sess) = guard.as_mut() else {
        return;
    };
    if sess.track_idx != usize::MAX || sess.plugin_id != marbble_id {
        return;
    }
    let (edits, preset) = sess.bridge.drain_edits();
    for (id, v) in edits {
        if let Some(idx) = info.params.iter().position(|p| p.id == id) {
            if let Some(slot) = params.get_mut(idx) {
                *slot = v;
            }
        }
    }
    if let Some(preset_name) = preset {
        if let Some(pr) = fx::factory_presets(marbble_id)
            .iter()
            .find(|pr| pr.name == preset_name)
        {
            for (idx, spec) in info.params.iter().enumerate() {
                if let Some(slot) = params.get_mut(idx) {
                    *slot = spec.default;
                }
            }
            for &(id, v) in pr.values {
                if let Some(idx) = info.params.iter().position(|p| p.id == id) {
                    if let Some(slot) = params.get_mut(idx) {
                        *slot = v;
                    }
                }
            }
        }
    }
}
