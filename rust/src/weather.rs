use crate::theme::WeatherScene;
use chrono::Local;
use egui::ColorImage;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct HourlyItem {
    pub hour: String,
    pub temp_c: i32,
    pub precip_prob: i32,
    pub icon: &'static str,
}

#[derive(Debug, Clone)]
pub struct DailyItem {
    pub day: String,
    pub condition: &'static str,
    pub icon: &'static str,
    pub low_c: i32,
    pub high_c: i32,
    pub precip_prob: i32,
}

#[derive(Debug, Clone)]
pub struct WeatherData {
    pub temp_c: i32,
    pub feels_like_c: i32,
    pub high_c: i32,
    pub low_c: i32,
    pub humidity: i32,
    pub wind_kmh: i32,
    pub precip_prob: i32,
    pub condition: &'static str,
    pub icon: &'static str,
    pub scene: WeatherScene,
    pub sunrise: String,
    pub sunset: String,
    pub updated_hm: String,
    pub hourly: Vec<HourlyItem>,
    pub daily: Vec<DailyItem>,
}

#[derive(Deserialize)]
struct OpenMeteoResp {
    current: Option<CurrentBlock>,
    hourly: Option<HourlyBlock>,
    daily: Option<DailyBlock>,
}

#[derive(Deserialize)]
struct CurrentBlock {
    time: Option<String>,
    temperature_2m: Option<f64>,
    apparent_temperature: Option<f64>,
    relative_humidity_2m: Option<f64>,
    wind_speed_10m: Option<f64>,
    weather_code: Option<i32>,
    is_day: Option<i32>,
}

#[derive(Deserialize)]
struct HourlyBlock {
    time: Option<Vec<String>>,
    temperature_2m: Option<Vec<f64>>,
    precipitation_probability: Option<Vec<f64>>,
    weather_code: Option<Vec<i32>>,
}

#[derive(Deserialize)]
struct DailyBlock {
    time: Option<Vec<String>>,
    weather_code: Option<Vec<i32>>,
    temperature_2m_max: Option<Vec<f64>>,
    temperature_2m_min: Option<Vec<f64>>,
    precipitation_probability_max: Option<Vec<f64>>,
    sunrise: Option<Vec<String>>,
    sunset: Option<Vec<String>>,
}

pub fn decode_code(code: i32) -> (&'static str, &'static str) {
    match code {
        0 => ("Clear Sky", "☀"),
        1 => ("Mainly Clear", "🌤"),
        2 => ("Partly Cloudy", "⛅"),
        3 => ("Overcast", "☁"),
        4..=48 => ("Foggy", "🌫"),
        49..=57 => ("Light Drizzle", "🌦"),
        58..=67 => ("Rain", "🌧"),
        68..=77 => ("Snow", "❄"),
        78..=82 => ("Rain Showers", "🌧"),
        _ => ("Thunderstorm", "⛈"),
    }
}

fn hm_from_iso(s: &str) -> String {
    if let Some(idx) = s.find('T') {
        s.get(idx + 1..idx + 6).unwrap_or("--:--").to_string()
    } else {
        s.chars().take(5).collect()
    }
}

fn to_mins(hm: &str) -> i32 {
    let mut parts = hm.split(':');
    let h: i32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let m: i32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    h * 60 + m
}

pub fn fetch_london_weather() -> Option<WeatherData> {
    let url = "https://api.open-meteo.com/v1/forecast?latitude=51.5074&longitude=-0.1278\
        &current=temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,weather_code,is_day\
        &hourly=temperature_2m,precipitation_probability,weather_code\
        &daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset\
        &timezone=Europe%2FLondon&forecast_days=5";

    let resp: OpenMeteoResp = ureq::get(url).call().ok()?.into_json().ok()?;
    let cur = resp.current?;
    let temp_c = cur.temperature_2m?.round() as i32;
    let feels_like_c = cur.apparent_temperature.unwrap_or(temp_c as f64).round() as i32;
    let humidity = cur.relative_humidity_2m.unwrap_or(0.0).round() as i32;
    let wind_kmh = cur.wind_speed_10m.unwrap_or(0.0).round() as i32;
    let wcode = cur.weather_code.unwrap_or(0);
    let is_day = cur.is_day.unwrap_or(1) == 1;
    let (condition, icon) = decode_code(wcode);

    let daily = resp.daily.as_ref();
    let high_c = daily
        .and_then(|d| d.temperature_2m_max.as_ref()?.first().copied())
        .unwrap_or(temp_c as f64)
        .round() as i32;
    let low_c = daily
        .and_then(|d| d.temperature_2m_min.as_ref()?.first().copied())
        .unwrap_or(temp_c as f64)
        .round() as i32;
    let precip_prob = daily
        .and_then(|d| d.precipitation_probability_max.as_ref()?.first().copied())
        .unwrap_or(0.0)
        .round() as i32;
    let sunrise = daily
        .and_then(|d| d.sunrise.as_ref()?.first())
        .map(|s| hm_from_iso(s))
        .unwrap_or_else(|| "07:30".to_string());
    let sunset = daily
        .and_then(|d| d.sunset.as_ref()?.first())
        .map(|s| hm_from_iso(s))
        .unwrap_or_else(|| "16:05".to_string());

    let updated_hm = Local::now().format("%H:%M").to_string();
    let now_m = to_mins(&updated_hm);
    let is_golden = (now_m - to_mins(&sunrise)).abs() <= 45 || (now_m - to_mins(&sunset)).abs() <= 45;

    let scene = match wcode {
        83..=99 => WeatherScene::Storm,
        49..=67 | 78..=82 => WeatherScene::Rain,
        68..=77 => WeatherScene::Snow,
        4..=48 => WeatherScene::Fog,
        3 => {
            if !is_day {
                WeatherScene::ClearNight
            } else {
                WeatherScene::Cloudy
            }
        }
        _ => {
            if is_golden {
                WeatherScene::Golden
            } else if is_day {
                WeatherScene::ClearDay
            } else {
                WeatherScene::ClearNight
            }
        }
    };

    // Hourly (next 8 hours)
    let mut hourly = Vec::new();
    if let Some(hb) = resp.hourly.as_ref() {
        let times = hb.time.as_deref().unwrap_or(&[]);
        let temps = hb.temperature_2m.as_deref().unwrap_or(&[]);
        let probs = hb.precipitation_probability.as_deref().unwrap_or(&[]);
        let codes = hb.weather_code.as_deref().unwrap_or(&[]);
        let prefix = cur.time.as_deref().and_then(|t| t.get(0..13)).unwrap_or("");
        let start = times
            .iter()
            .position(|t| !prefix.is_empty() && t.starts_with(prefix))
            .unwrap_or(0);
        for i in start..(start + 8).min(times.len()) {
            let (_, ic) = decode_code(*codes.get(i).unwrap_or(&0));
            hourly.push(HourlyItem {
                hour: if i == start {
                    "Now".to_string()
                } else {
                    hm_from_iso(&times[i])
                },
                temp_c: temps.get(i).copied().unwrap_or(temp_c as f64).round() as i32,
                precip_prob: probs.get(i).copied().unwrap_or(0.0).round() as i32,
                icon: ic,
            });
        }
    }

    // 5-day forecast
    let mut daily_items = Vec::new();
    if let Some(db) = daily {
        let times = db.time.as_deref().unwrap_or(&[]);
        let highs = db.temperature_2m_max.as_deref().unwrap_or(&[]);
        let lows = db.temperature_2m_min.as_deref().unwrap_or(&[]);
        let probs = db.precipitation_probability_max.as_deref().unwrap_or(&[]);
        let codes = db.weather_code.as_deref().unwrap_or(&[]);
        for i in 0..5.min(times.len()) {
            let (cond, ic) = decode_code(*codes.get(i).unwrap_or(&0));
            let day = match i {
                0 => "Today".to_string(),
                1 => "Tomorrow".to_string(),
                _ => {
                    chrono::NaiveDate::parse_from_str(&times[i], "%Y-%m-%d")
                        .map(|d| d.format("%a %d %b").to_string())
                        .unwrap_or_else(|_| times[i].clone())
                }
            };
            daily_items.push(DailyItem {
                day,
                condition: cond,
                icon: ic,
                low_c: lows.get(i).copied().unwrap_or(low_c as f64).round() as i32,
                high_c: highs.get(i).copied().unwrap_or(high_c as f64).round() as i32,
                precip_prob: probs.get(i).copied().unwrap_or(0.0).round() as i32,
            });
        }
    }

    Some(WeatherData {
        temp_c,
        feels_like_c,
        high_c,
        low_c,
        humidity,
        wind_kmh,
        precip_prob,
        condition,
        icon,
        scene,
        sunrise,
        sunset,
        updated_hm,
        hourly,
        daily: daily_items,
    })
}

pub fn scene_photo_url(scene: WeatherScene) -> &'static str {
    match scene {
        WeatherScene::ClearDay | WeatherScene::Auto => {
            "https://images.unsplash.com/photo-1513002749550-c59d786b8e6c?auto=format&fit=crop&w=2560&q=90"
        }
        WeatherScene::Golden => {
            "https://images.unsplash.com/photo-1495616811223-4d98c6e9c869?auto=format&fit=crop&w=2560&q=90"
        }
        WeatherScene::ClearNight => {
            "https://images.unsplash.com/photo-1519681393784-d120267933ba?auto=format&fit=crop&w=2560&q=90"
        }
        WeatherScene::Cloudy => {
            "https://images.unsplash.com/photo-1534088568595-a066f410bcda?auto=format&fit=crop&w=2560&q=90"
        }
        WeatherScene::Fog => {
            "https://images.unsplash.com/photo-1487621167305-5d248087c724?auto=format&fit=crop&w=2560&q=90"
        }
        WeatherScene::Rain => {
            "https://images.unsplash.com/photo-1515694346937-94d85e41e6f0?auto=format&fit=crop&w=2560&q=90"
        }
        WeatherScene::Storm => {
            "https://images.unsplash.com/photo-1605727216801-e27ce1d0cc28?auto=format&fit=crop&w=2560&q=90"
        }
        WeatherScene::Snow => {
            "https://images.unsplash.com/photo-1491002052546-bf38f186af56?auto=format&fit=crop&w=2560&q=90"
        }
        WeatherScene::LiveUkSat => {
            "https://view.eumetsat.int/geoserver/wms?service=WMS&version=1.3.0&request=GetMap&layers=mtg_fd:rgb_geocolour&bbox=35,-16,61,30&width=2560&height=1440&crs=EPSG:4326&format=image/png"
        }
        WeatherScene::LiveEarth => {
            "https://cdn.star.nesdis.noaa.gov/GOES19/ABI/FD/GEOCOLOR/1808x1808.jpg"
        }
    }
}

pub fn fetch_wallpaper_image(scene: WeatherScene, blur_level: u8) -> Option<ColorImage> {
    let is_live_sat = matches!(scene, WeatherScene::LiveUkSat | WeatherScene::LiveEarth);
    let cache_path = format!(
        "/data/user/0/com.astryxtv.rustlauncher/cache/wp_{scene:?}.bin"
    );

    let bytes = if !is_live_sat {
        if let Ok(cached) = std::fs::read(&cache_path) {
            if !cached.is_empty() {
                cached
            } else {
                let resp = ureq::get(scene_photo_url(scene)).call().ok()?;
                let mut b = Vec::new();
                std::io::Read::read_to_end(&mut resp.into_reader(), &mut b).ok()?;
                let _ = std::fs::write(&cache_path, &b);
                b
            }
        } else {
            let resp = ureq::get(scene_photo_url(scene)).call().ok()?;
            let mut b = Vec::new();
            std::io::Read::read_to_end(&mut resp.into_reader(), &mut b).ok()?;
            let _ = std::fs::write(&cache_path, &b);
            b
        }
    } else {
        let resp = ureq::get(scene_photo_url(scene)).call().ok()?;
        let mut b = Vec::new();
        std::io::Read::read_to_end(&mut resp.into_reader(), &mut b).ok()?;
        b
    };

    let mut img = image::load_from_memory(&bytes).ok()?;
    // Supersample 4K/QHD source cleanly with CatmullRom to crisp native 1920x1080 (zero JPEG artifacts!)
    if img.width() > 1920 || img.height() > 1080 {
        img = img.resize(1920, 1080, image::imageops::FilterType::CatmullRom);
    }
    if blur_level > 0 {
        let w = img.width();
        let h = img.height();
        let small = img.resize(w / 4, h / 4, image::imageops::FilterType::Triangle);
        let sigma = (blur_level as f32) * 1.4;
        let blurred = small.blur(sigma);
        img = blurred.resize_exact(w / 2, h / 2, image::imageops::FilterType::Triangle);
    }
    let mut rgba = img.to_rgba8();
    let w = rgba.width() as usize;
    let h = rgba.height() as usize;
    let raw = rgba.as_mut();
    if scene == WeatherScene::LiveUkSat {
        // Gentle edge vignette only so MTG-I1 1km clouds & city lights stay vivid and razor-sharp
        for y in 0..h {
            let t = y as f32 / (h.max(1) as f32);
            let mul = 0.88 - t * 0.20;
            let row = y * w * 4;
            for x in 0..w {
                let idx = row + x * 4;
                raw[idx] = (raw[idx] as f32 * mul) as u8;
                raw[idx + 1] = (raw[idx + 1] as f32 * mul) as u8;
                raw[idx + 2] = (raw[idx + 2] as f32 * mul) as u8;
                raw[idx + 3] = 255;
            }
        }
    } else if scene != WeatherScene::LiveEarth {
        // Pre-bake atmospheric contrast vignette into sky wallpapers once off-thread
        for y in 0..h {
            let t = y as f32 / (h.max(1) as f32);
            let mul = 0.64 - t * 0.28;
            let row = y * w * 4;
            for x in 0..w {
                let idx = row + x * 4;
                raw[idx] = (raw[idx] as f32 * mul + 5.0) as u8;
                raw[idx + 1] = (raw[idx + 1] as f32 * mul + 7.0) as u8;
                raw[idx + 2] = (raw[idx + 2] as f32 * mul + 14.0) as u8;
                raw[idx + 3] = 255;
            }
        }
    }
    Some(ColorImage::from_rgba_unmultiplied([w, h], raw))
}
