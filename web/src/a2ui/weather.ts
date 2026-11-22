export type WeatherIconKey =
  | 'sun'
  | 'partly'
  | 'cloud'
  | 'fog'
  | 'drizzle'
  | 'rain'
  | 'snow'
  | 'storm';

export interface HourlyForecastItem {
  hourLabel: string;
  tempC: number;
  precipProb: number;
  iconKey: WeatherIconKey;
}

export interface DailyForecastItem {
  dayLabel: string;
  highC: number;
  lowC: number;
  precipProb: number;
  condition: string;
  iconKey: WeatherIconKey;
}

export interface DetailedWeather {
  tempC: number;
  feelsLikeC: number;
  highC: number;
  lowC: number;
  humidity: number;
  windKmh: number;
  precipProb: number;
  condition: string;
  iconKey: WeatherIconKey;
  isDay: boolean;
  timeOfDay: 'day' | 'golden' | 'night';
  sunrise: string;
  sunset: string;
  updatedTime: string;
  summaryText: string;
  hourly: HourlyForecastItem[];
  daily: DailyForecastItem[];
  updatedAt: number;
}

const CACHE_KEY = 'astryxtv:weather_london_v3';
const TTL_MS = 10 * 60 * 1000;

function decodeWeather(code: number): {
  condition: string;
  iconKey: WeatherIconKey;
} {
  if (code === 0) return { condition: 'Clear Sky', iconKey: 'sun' };
  if (code === 1) return { condition: 'Mainly Clear', iconKey: 'partly' };
  if (code === 2) return { condition: 'Partly Cloudy', iconKey: 'partly' };
  if (code === 3) return { condition: 'Overcast', iconKey: 'cloud' };
  if (code <= 48) return { condition: 'Foggy', iconKey: 'fog' };
  if (code <= 57) return { condition: 'Light Drizzle', iconKey: 'drizzle' };
  if (code <= 67) return { condition: 'Rain', iconKey: 'rain' };
  if (code <= 77) return { condition: 'Snow', iconKey: 'snow' };
  if (code <= 82) return { condition: 'Rain Showers', iconKey: 'rain' };
  return { condition: 'Thunderstorm', iconKey: 'storm' };
}

function formatHmFromIso(iso: string | undefined): string {
  if (!iso) return '--:--';
  const idx = iso.indexOf('T');
  return idx >= 0 ? iso.slice(idx + 1, idx + 6) : iso.slice(0, 5);
}

function formatDayLabel(isoDate: string, idx: number): string {
  if (idx === 0) return 'Today';
  if (idx === 1) return 'Tomorrow';
  try {
    const d = new Date(`${isoDate}T12:00:00Z`);
    return new Intl.DateTimeFormat('en-GB', { weekday: 'short', day: 'numeric', month: 'short' }).format(d);
  } catch {
    return isoDate;
  }
}

export function getCachedWeather(): DetailedWeather | null {
  try {
    const raw = window.localStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    return JSON.parse(raw) as DetailedWeather;
  } catch {
    return null;
  }
}

export async function fetchDetailedLondonWeather(force = false): Promise<DetailedWeather | null> {
  if (!force) {
    const cached = getCachedWeather();
    if (cached && Date.now() - cached.updatedAt < TTL_MS) {
      return cached;
    }
  }

  try {
    const url =
      'https://api.open-meteo.com/v1/forecast?latitude=51.5074&longitude=-0.1278' +
      '&current=temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,weather_code,is_day' +
      '&hourly=temperature_2m,precipitation_probability,weather_code' +
      '&daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset' +
      '&timezone=Europe%2FLondon&forecast_days=5';
    const res = await fetch(url);
    if (!res.ok) return getCachedWeather();
    const data = await res.json();

    const cur = data?.current ?? {};
    const daily = data?.daily ?? {};
    const hourlyRaw = data?.hourly ?? {};
    const tempC = Math.round(Number(cur.temperature_2m));
    if (Number.isNaN(tempC)) return getCachedWeather();

    const feelsLikeC = Math.round(Number(cur.apparent_temperature ?? tempC));
    const humidity = Math.round(Number(cur.relative_humidity_2m ?? 0));
    const windKmh = Math.round(Number(cur.wind_speed_10m ?? 0));
    const highC = Math.round(Number(daily.temperature_2m_max?.[0] ?? tempC));
    const lowC = Math.round(Number(daily.temperature_2m_min?.[0] ?? tempC));
    const precipProb = Math.round(Number(daily.precipitation_probability_max?.[0] ?? 0));
    const sunrise = formatHmFromIso(daily.sunrise?.[0]);
    const sunset = formatHmFromIso(daily.sunset?.[0]);
    const isDay = Number(cur.is_day ?? 1) === 1;
    const { condition, iconKey } = decodeWeather(Number(cur.weather_code ?? 0));

    const nowHm = new Intl.DateTimeFormat('en-GB', {
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
      timeZone: 'Europe/London',
    }).format(new Date());

    const toMins = (hm: string) => {
      const [h, m] = hm.split(':').map(Number);
      return (h || 0) * 60 + (m || 0);
    };
    const nowM = toMins(nowHm);
    const riseM = toMins(sunrise);
    const setM = toMins(sunset);
    const isGolden = Math.abs(nowM - riseM) <= 45 || Math.abs(nowM - setM) <= 45;
    const timeOfDay: DetailedWeather['timeOfDay'] = isGolden ? 'golden' : isDay ? 'day' : 'night';

    // Next 8 hours from current hour
    const curHourPrefix = typeof cur.time === 'string' ? cur.time.slice(0, 13) : '';
    const hTimes: string[] = Array.isArray(hourlyRaw.time) ? hourlyRaw.time : [];
    let startIdx = curHourPrefix ? hTimes.findIndex((t) => t.startsWith(curHourPrefix)) : 0;
    if (startIdx < 0) startIdx = 0;

    const hourly: HourlyForecastItem[] = [];
    for (let i = startIdx; i < Math.min(hTimes.length, startIdx + 8); i++) {
      const dec = decodeWeather(Number(hourlyRaw.weather_code?.[i] ?? 0));
      hourly.push({
        hourLabel: i === startIdx ? 'Now' : formatHmFromIso(hTimes[i]),
        tempC: Math.round(Number(hourlyRaw.temperature_2m?.[i] ?? tempC)),
        precipProb: Math.round(Number(hourlyRaw.precipitation_probability?.[i] ?? 0)),
        iconKey: dec.iconKey,
      });
    }

    const dTimes: string[] = Array.isArray(daily.time) ? daily.time : [];
    const dailyItems: DailyForecastItem[] = dTimes.slice(0, 5).map((iso, idx) => {
      const dec = decodeWeather(Number(daily.weather_code?.[idx] ?? 0));
      return {
        dayLabel: formatDayLabel(iso, idx),
        highC: Math.round(Number(daily.temperature_2m_max?.[idx] ?? tempC)),
        lowC: Math.round(Number(daily.temperature_2m_min?.[idx] ?? tempC)),
        precipProb: Math.round(Number(daily.precipitation_probability_max?.[idx] ?? 0)),
        condition: dec.condition,
        iconKey: dec.iconKey,
      };
    });

    const result: DetailedWeather = {
      tempC,
      feelsLikeC,
      highC,
      lowC,
      humidity,
      windKmh,
      precipProb,
      condition,
      iconKey,
      isDay,
      timeOfDay,
      sunrise,
      sunset,
      updatedTime: nowHm,
      summaryText: `${tempC}°C ${condition}`,
      hourly,
      daily: dailyItems,
      updatedAt: Date.now(),
    };

    try {
      window.localStorage.setItem(CACHE_KEY, JSON.stringify(result));
    } catch {
      // ignore storage quota
    }
    return result;
  } catch {
    return getCachedWeather();
  }
}
