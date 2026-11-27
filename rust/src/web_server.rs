use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub const WEB_COMPANION_PORT: u16 = 8787;

#[derive(Debug, Clone)]
pub enum WebCmd {
    SetCity(String),
    ClearRam,
    ToggleOverlay,
    SetScene(String),
    SetTheme(String),
    CustomWallpaperUploaded(String),
    PushA2ui(String),
    ImportConfigJson(String),
    TriggerSleep,
    SetGeminiKey(String),
    SetGeminiModel(String),
    AskGemini(String),
    GeminiA2ui(String),
}

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width,initial-scale=1"/>
<title>Astryx TV Companion</title>
<style>
  *{box-sizing:border-box;margin:0;padding:0}
  body{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;background:radial-gradient(circle at 20% 10%,#122347,#070b14 60%);color:#f1f5f9;min-height:100vh;padding:20px 16px 40px}
  .wrap{max-width:520px;margin:0 auto;display:flex;flex-direction:column;gap:14px}
  h1{font-size:24px;font-weight:800;background:linear-gradient(120deg,#fff,#38bdf8,#a855f7);-webkit-background-clip:text;-webkit-text-fill-color:transparent}
  .sub{font-size:12.5px;color:#94a3b8;margin-top:2px}
  .card{background:rgba(15,23,42,.72);border:1px solid rgba(255,255,255,.14);border-radius:18px;padding:15px;backdrop-filter:blur(16px);display:flex;flex-direction:column;gap:10px}
  .card h2{font-size:13px;text-transform:uppercase;letter-spacing:.08em;color:#38bdf8;font-weight:800}
  .row{display:flex;gap:8px;flex-wrap:wrap}
  input,textarea,select{width:100%;background:rgba(255,255,255,.07);border:1px solid rgba(255,255,255,.16);border-radius:12px;color:#fff;padding:10px 12px;font-size:14px;outline:none}
  button{background:linear-gradient(135deg,#38bdf8,#6366f1);color:#fff;border:none;border-radius:12px;padding:10px 14px;font-weight:700;font-size:13.5px;cursor:pointer;flex:1;min-width:110px}
  button.sec{background:rgba(255,255,255,.1);border:1px solid rgba(255,255,255,.18)}
  #msg{position:fixed;bottom:16px;left:50%;transform:translateX(-50%);background:#0f172a;border:1px solid #38bdf8;padding:10px 16px;border-radius:999px;font-size:13px;font-weight:700;display:none}
</style>
</head>
<body>
<div class="wrap">
  <div>
    <h1>Astryx TV Companion</h1>
    <div class="sub">Local Wi-Fi Remote &amp; 4K Configurator</div>
  </div>

  <div class="card">
    <h2>Quick TV Actions</h2>
    <div class="row">
      <button onclick="post('/api/clear-ram','')">⚡ Clear RAM</button>
      <button class="sec" onclick="post('/api/toggle-overlay','')">📊 HUD Overlay</button>
      <button class="sec" onclick="post('/api/sleep','')">🌙 Sleep TV</button>
    </div>
  </div>

  <div class="card">
    <h2>✨ Gemini AI (TV Assistant &amp; A2UI)</h2>
    <div class="row">
      <input id="gkey" type="password" placeholder="Paste Gemini API key (AIza...)"/>
      <button class="sec" onclick="post('/api/gemini-key',document.getElementById('gkey').value)">Save Key</button>
    </div>
    <div class="row">
      <select id="gmodel" onchange="post('/api/gemini-model',this.value)">
        <option value="gemini-4-hydrogen">gemini-4-hydrogen (Fast Balanced)</option>
        <option value="gemini-4-neon">gemini-4-neon (Frontier)</option>
        <option value="gemini-3.5-flash-lite">gemini-3.5-flash-lite (Ultra Fast)</option>
      </select>
    </div>
    <div class="row">
      <input id="gprompt" placeholder="Ask Gemini or describe a TV dashboard shelf..."/>
    </div>
    <div class="row">
      <button onclick="post('/api/gemini-ask',document.getElementById('gprompt').value)">✨ Ask on TV</button>
      <button class="sec" onclick="post('/api/gemini-a2ui',document.getElementById('gprompt').value)">🧩 Build A2UI Shelf</button>
    </div>
  </div>

  <div class="card">
    <h2>Weather City</h2>
    <div class="row">
      <input id="city" placeholder="e.g. London, Manchester, New York, Tokyo"/>
      <button onclick="post('/api/city',document.getElementById('city').value)">Set City</button>
    </div>
  </div>

  <div class="card">
    <h2>Theme &amp; Live Wallpaper</h2>
    <div class="row">
      <button class="sec" onclick="post('/api/theme','Gothic')">Gothic</button>
      <button class="sec" onclick="post('/api/theme','Y2k')">Y2K</button>
      <button class="sec" onclick="post('/api/theme','Matcha')">Matcha</button>
      <button class="sec" onclick="post('/api/theme','Neutral')">Neutral</button>
    </div>
    <div class="row">
      <button class="sec" onclick="post('/api/scene','LiveUkSat')">UK Sat 1km</button>
      <button class="sec" onclick="post('/api/scene','AuroraBorealis')">Aurora Shader</button>
      <button class="sec" onclick="post('/api/scene','LiquidMesh')">Liquid Mesh</button>
      <button class="sec" onclick="post('/api/scene','DeepSpace')">Deep Space</button>
    </div>
  </div>

  <div class="card">
    <h2>Upload Custom 4K Wallpaper</h2>
    <input type="file" id="wp" accept="image/*"/>
    <button onclick="uploadWp()">Upload &amp; Apply to TV</button>
  </div>

  <div class="card">
    <h2>Backup &amp; Restore Config</h2>
    <div class="row">
      <button class="sec" onclick="location.href='/api/config'">⬇ Download Backup JSON</button>
      <button class="sec" onclick="restoreCfg()">⬆ Restore JSON</button>
    </div>
    <textarea id="cfg" rows="3" placeholder="Paste exported config JSON here to restore..."></textarea>
  </div>
</div>
<div id="msg"></div>
<script>
function toast(t){const m=document.getElementById('msg');m.textContent=t;m.style.display='block';setTimeout(()=>m.style.display='none',2400);}
async function post(url,body){
  const r=await fetch(url,{method:'POST',body});
  toast(await r.text());
}
async function uploadWp(){
  const f=document.getElementById('wp').files[0];
  if(!f){toast('Choose an image first');return;}
  toast('Uploading wallpaper...');
  const buf=await f.arrayBuffer();
  const r=await fetch('/api/wallpaper',{method:'POST',body:buf});
  toast(await r.text());
}
async function restoreCfg(){
  const txt=document.getElementById('cfg').value.trim();
  if(!txt){toast('Paste JSON first');return;}
  await post('/api/import-config',txt);
}
</script>
</body>
</html>
"#;

pub fn spawn(
    tx: Sender<WebCmd>,
    ctx: egui::Context,
    cache_dir: String,
    config_snapshot: Arc<Mutex<String>>,
) {
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(("0.0.0.0", WEB_COMPANION_PORT)) {
            Ok(l) => l,
            Err(e) => {
                log::warn!("Web Companion bind failed on port {WEB_COMPANION_PORT}: {e}");
                return;
            }
        };
        log::info!("Astryx Phone Web Companion listening on 0.0.0.0:{WEB_COMPANION_PORT}");
        for stream in listener.incoming().flatten() {
            let tx_c = tx.clone();
            let ctx_c = ctx.clone();
            let dir_c = cache_dir.clone();
            let snap_c = config_snapshot.clone();
            std::thread::spawn(move || {
                handle_client(stream, &tx_c, &ctx_c, &dir_c, &snap_c);
            });
        }
    });
}

fn handle_client(
    mut stream: TcpStream,
    tx: &Sender<WebCmd>,
    ctx: &egui::Context,
    cache_dir: &str,
    config_snapshot: &Arc<Mutex<String>>,
) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });

    let mut req_line = String::new();
    if reader.read_line(&mut req_line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let path = parts.next().unwrap_or("/");

    let mut content_len: usize = 0;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).unwrap_or(0) == 0 {
            break;
        }
        let trimmed = h.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            if k.eq_ignore_ascii_case("Content-Length") {
                content_len = v.trim().parse().unwrap_or(0);
            }
        }
    }

    // Cap uploads at 16 MB for safety
    let body_len = content_len.min(16 * 1024 * 1024);
    let mut body = vec![0u8; body_len];
    if body_len > 0 && reader.read_exact(&mut body).is_err() {
        return;
    }

    match (method, path) {
        ("GET", "/") => {
            respond(&mut stream, "200 OK", "text/html; charset=utf-8", INDEX_HTML.as_bytes());
        }
        ("GET", "/api/config") => {
            let json = config_snapshot
                .lock()
                .map(|g| g.clone())
                .unwrap_or_else(|_| "{}".to_string());
            let hdr = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Disposition: attachment; filename=\"astryx_backup.json\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                json.len()
            );
            let _ = stream.write_all(hdr.as_bytes());
            let _ = stream.write_all(json.as_bytes());
        }
        ("POST", "/api/clear-ram") => {
            let _ = tx.send(WebCmd::ClearRam);
            ctx.request_repaint();
            respond(&mut stream, "200 OK", "text/plain", b"Clearing TV RAM...");
        }
        ("POST", "/api/toggle-overlay") => {
            let _ = tx.send(WebCmd::ToggleOverlay);
            ctx.request_repaint();
            respond(&mut stream, "200 OK", "text/plain", b"Toggled HUD Overlay");
        }
        ("POST", "/api/sleep") => {
            let _ = tx.send(WebCmd::TriggerSleep);
            ctx.request_repaint();
            respond(&mut stream, "200 OK", "text/plain", b"Starting TV Ambient Sleep");
        }
        ("POST", "/api/city") => {
            let city = String::from_utf8_lossy(&body).trim().to_string();
            if city.is_empty() {
                respond(&mut stream, "400 Bad Request", "text/plain", b"Enter a city name");
            } else {
                let _ = tx.send(WebCmd::SetCity(city.clone()));
                ctx.request_repaint();
                respond(
                    &mut stream,
                    "200 OK",
                    "text/plain",
                    format!("Updating weather to {city}...").as_bytes(),
                );
            }
        }
        ("POST", "/api/theme") => {
            let t = String::from_utf8_lossy(&body).trim().to_string();
            let _ = tx.send(WebCmd::SetTheme(t.clone()));
            ctx.request_repaint();
            respond(&mut stream, "200 OK", "text/plain", format!("Theme -> {t}").as_bytes());
        }
        ("POST", "/api/scene") => {
            let s = String::from_utf8_lossy(&body).trim().to_string();
            let _ = tx.send(WebCmd::SetScene(s.clone()));
            ctx.request_repaint();
            respond(&mut stream, "200 OK", "text/plain", format!("Wallpaper -> {s}").as_bytes());
        }
        ("POST", "/api/wallpaper") => {
            if body.len() < 1024 {
                respond(&mut stream, "400 Bad Request", "text/plain", b"Image too small");
            } else {
                let mut p = PathBuf::from(cache_dir);
                let _ = fs::create_dir_all(&p);
                p.push("custom_wallpaper.img");
                match fs::write(&p, &body) {
                    Ok(_) => {
                        let path_s = p.to_string_lossy().to_string();
                        let _ = tx.send(WebCmd::CustomWallpaperUploaded(path_s));
                        ctx.request_repaint();
                        respond(&mut stream, "200 OK", "text/plain", b"Applied 4K custom wallpaper!");
                    }
                    Err(_) => respond(&mut stream, "500 Internal Server Error", "text/plain", b"Write failed"),
                }
            }
        }
        ("POST", "/api/a2ui") => {
            let payload = String::from_utf8_lossy(&body).to_string();
            let _ = tx.send(WebCmd::PushA2ui(payload));
            ctx.request_repaint();
            respond(&mut stream, "200 OK", "text/plain", b"Pushed A2UI surface to TV");
        }
        ("POST", "/api/gemini-key") => {
            let key = String::from_utf8_lossy(&body).trim().to_string();
            let _ = tx.send(WebCmd::SetGeminiKey(key));
            ctx.request_repaint();
            respond(&mut stream, "200 OK", "text/plain", b"Saved Gemini API key on TV");
        }
        ("POST", "/api/gemini-model") => {
            let m = String::from_utf8_lossy(&body).trim().to_string();
            let _ = tx.send(WebCmd::SetGeminiModel(m.clone()));
            ctx.request_repaint();
            respond(&mut stream, "200 OK", "text/plain", format!("Gemini model -> {m}").as_bytes());
        }
        ("POST", "/api/gemini-ask") => {
            let prompt = String::from_utf8_lossy(&body).trim().to_string();
            if prompt.is_empty() {
                respond(&mut stream, "400 Bad Request", "text/plain", b"Enter a prompt first");
            } else {
                let _ = tx.send(WebCmd::AskGemini(prompt));
                ctx.request_repaint();
                respond(&mut stream, "200 OK", "text/plain", b"Asking Gemini on TV...");
            }
        }
        ("POST", "/api/gemini-a2ui") => {
            let prompt = String::from_utf8_lossy(&body).trim().to_string();
            if prompt.is_empty() {
                respond(&mut stream, "400 Bad Request", "text/plain", b"Enter a prompt first");
            } else {
                let _ = tx.send(WebCmd::GeminiA2ui(prompt));
                ctx.request_repaint();
                respond(&mut stream, "200 OK", "text/plain", b"Generating A2UI surface with Gemini...");
            }
        }
        ("POST", "/api/import-config") => {
            let json = String::from_utf8_lossy(&body).to_string();
            let _ = tx.send(WebCmd::ImportConfigJson(json));
            ctx.request_repaint();
            respond(&mut stream, "200 OK", "text/plain", b"Restored backup config!");
        }
        _ => {
            respond(&mut stream, "404 Not Found", "text/plain", b"Not Found");
        }
    }
}

fn respond(stream: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) {
    let hdr = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(hdr.as_bytes());
    let _ = stream.write_all(body);
}
