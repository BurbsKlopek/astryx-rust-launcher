import React, { useEffect, useMemo, useRef } from 'react';
import type { DetailedWeather } from '../a2ui/weather';
import type { WeatherSceneOverride } from '../theme/themes';

type ResolvedScene =
  | 'clear-day'
  | 'golden'
  | 'clear-night'
  | 'cloudy'
  | 'fog'
  | 'rain'
  | 'storm'
  | 'snow';

interface SceneConfig {
  photoUrl: string;
  fallbackGradient: string;
  scrim: string;
  effect: 'sun' | 'stars' | 'mist' | 'rain' | 'storm' | 'snow';
}

// High-resolution photorealistic sky & weather photography
const SCENES: Record<ResolvedScene, SceneConfig> = {
  'clear-day': {
    photoUrl:
      'https://images.unsplash.com/photo-1513002749550-c59d786b8e6c?auto=format&fit=crop&w=1280&q=75',
    fallbackGradient: 'linear-gradient(180deg, #1d4ed8 0%, #38bdf8 55%, #0f172a 100%)',
    scrim: 'linear-gradient(180deg, rgba(6,12,26,0.46) 0%, rgba(6,12,26,0.34) 45%, rgba(5,8,18,0.72) 100%)',
    effect: 'sun',
  },
  golden: {
    photoUrl:
      'https://images.unsplash.com/photo-1495616811223-4d98c6e9c869?auto=format&fit=crop&w=1280&q=75',
    fallbackGradient: 'linear-gradient(180deg, #1e1b4b 0%, #c2410c 52%, #0c0a14 100%)',
    scrim: 'linear-gradient(180deg, rgba(12,8,20,0.42) 0%, rgba(16,10,18,0.32) 48%, rgba(6,6,14,0.74) 100%)',
    effect: 'sun',
  },
  'clear-night': {
    photoUrl:
      'https://images.unsplash.com/photo-1519681393784-d120267933ba?auto=format&fit=crop&w=1280&q=75',
    fallbackGradient: 'linear-gradient(180deg, #030712 0%, #0f172a 55%, #020617 100%)',
    scrim: 'linear-gradient(180deg, rgba(4,6,14,0.44) 0%, rgba(4,7,18,0.28) 50%, rgba(4,6,12,0.72) 100%)',
    effect: 'stars',
  },
  cloudy: {
    photoUrl:
      'https://images.unsplash.com/photo-1534088568595-a066f410bcda?auto=format&fit=crop&w=1280&q=75',
    fallbackGradient: 'linear-gradient(180deg, #1e293b 0%, #334155 50%, #0f172a 100%)',
    scrim: 'linear-gradient(180deg, rgba(8,12,22,0.5) 0%, rgba(8,12,22,0.36) 48%, rgba(6,8,16,0.76) 100%)',
    effect: 'mist',
  },
  fog: {
    photoUrl:
      'https://images.unsplash.com/photo-1487621167305-5d248087c724?auto=format&fit=crop&w=1280&q=75',
    fallbackGradient: 'linear-gradient(180deg, #334155 0%, #475569 50%, #0f172a 100%)',
    scrim: 'linear-gradient(180deg, rgba(8,12,20,0.48) 0%, rgba(10,14,22,0.36) 50%, rgba(6,8,14,0.76) 100%)',
    effect: 'mist',
  },
  rain: {
    photoUrl:
      'https://images.unsplash.com/photo-1515694346937-94d85e41e6f0?auto=format&fit=crop&w=1280&q=75',
    fallbackGradient: 'linear-gradient(180deg, #0f172a 0%, #1e293b 52%, #020617 100%)',
    scrim: 'linear-gradient(180deg, rgba(5,10,20,0.52) 0%, rgba(5,10,20,0.38) 50%, rgba(4,7,14,0.78) 100%)',
    effect: 'rain',
  },
  storm: {
    photoUrl:
      'https://images.unsplash.com/photo-1605727216801-e27ce1d0cc28?auto=format&fit=crop&w=1280&q=75',
    fallbackGradient: 'linear-gradient(180deg, #090d1a 0%, #1e1b4b 50%, #030712 100%)',
    scrim: 'linear-gradient(180deg, rgba(5,7,16,0.52) 0%, rgba(8,8,20,0.36) 50%, rgba(4,6,14,0.78) 100%)',
    effect: 'storm',
  },
  snow: {
    photoUrl:
      'https://images.unsplash.com/photo-1491002052546-bf38f186af56?auto=format&fit=crop&w=1280&q=75',
    fallbackGradient: 'linear-gradient(180deg, #1e293b 0%, #475569 50%, #0f172a 100%)',
    scrim: 'linear-gradient(180deg, rgba(8,14,26,0.46) 0%, rgba(8,14,26,0.34) 50%, rgba(5,9,18,0.74) 100%)',
    effect: 'snow',
  },
};

function resolveScene(
  weather: DetailedWeather | null,
  override: WeatherSceneOverride,
): ResolvedScene {
  if (override && override !== 'auto') return override;
  if (!weather) {
    const hour = new Date().getHours();
    return hour >= 6 && hour < 19 ? 'clear-day' : 'clear-night';
  }
  switch (weather.iconKey) {
    case 'storm':
      return 'storm';
    case 'rain':
    case 'drizzle':
      return 'rain';
    case 'snow':
      return 'snow';
    case 'fog':
      return 'fog';
    case 'cloud':
      return weather.timeOfDay === 'night' ? 'clear-night' : 'cloudy';
    case 'partly':
      return weather.timeOfDay === 'golden'
        ? 'golden'
        : weather.timeOfDay === 'night'
          ? 'clear-night'
          : 'cloudy';
    case 'sun':
    default:
      return weather.timeOfDay === 'golden'
        ? 'golden'
        : weather.timeOfDay === 'night'
          ? 'clear-night'
          : 'clear-day';
  }
}

interface Props {
  weather: DetailedWeather | null;
  sceneOverride: WeatherSceneOverride;
}

export const WeatherWallpaper: React.FC<Props> = ({ weather, sceneOverride }) => {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const sceneKey = useMemo(
    () => resolveScene(weather, sceneOverride),
    [weather, sceneOverride],
  );
  const scene = SCENES[sceneKey];

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    let rafId = 0;
    let width = (canvas.width = window.innerWidth);
    let height = (canvas.height = window.innerHeight);

    const onResize = () => {
      width = canvas.width = window.innerWidth;
      height = canvas.height = window.innerHeight;
    };
    window.addEventListener('resize', onResize);

    const effect = scene.effect;
    const count =
      effect === 'rain' || effect === 'storm'
        ? 75
        : effect === 'snow'
          ? 55
          : effect === 'stars'
            ? 50
            : 0;

    const particles = Array.from({ length: count }, () => ({
      x: Math.random() * width,
      y: Math.random() * height,
      speed:
        effect === 'rain' || effect === 'storm'
          ? 9 + Math.random() * 10
          : effect === 'snow'
            ? 0.6 + Math.random() * 1.4
            : 0,
      len: 12 + Math.random() * 18,
      r: effect === 'snow' ? 1.5 + Math.random() * 2.8 : 0.8 + Math.random() * 1.4,
      alpha: 0.25 + Math.random() * 0.55,
      phase: Math.random() * Math.PI * 2,
    }));

    let flashAlpha = 0;
    let nextFlashAt = performance.now() + 4500 + Math.random() * 6000;
    let lastFrame = 0;

    const draw = (now: number) => {
      rafId = window.requestAnimationFrame(draw);
      if (document.hidden) return;
      // Throttle to ~30fps on TV WebView so D-pad navigation stays silky at 60fps
      if (now - lastFrame < 32) return;
      lastFrame = now;

      ctx.clearRect(0, 0, width, height);

      if (effect === 'rain' || effect === 'storm') {
        ctx.lineCap = 'round';
        for (const p of particles) {
          ctx.strokeStyle = `rgba(210, 232, 255, ${p.alpha * 0.55})`;
          ctx.lineWidth = 1.1;
          ctx.beginPath();
          ctx.moveTo(p.x, p.y);
          ctx.lineTo(p.x - 3.5, p.y + p.len);
          ctx.stroke();

          p.x -= 2.2;
          p.y += p.speed;
          if (p.y > height + 20) {
            p.y = -20;
            p.x = Math.random() * (width + 120);
          }
        }

        if (effect === 'storm') {
          if (now >= nextFlashAt) {
            flashAlpha = 0.32;
            nextFlashAt = now + 5500 + Math.random() * 8000;
          }
          if (flashAlpha > 0.005) {
            ctx.fillStyle = `rgba(215, 228, 255, ${flashAlpha})`;
            ctx.fillRect(0, 0, width, height);
            flashAlpha *= 0.84;
          }
        }
      } else if (effect === 'snow') {
        for (const p of particles) {
          ctx.fillStyle = `rgba(255, 255, 255, ${p.alpha * 0.8})`;
          ctx.beginPath();
          ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
          ctx.fill();

          p.y += p.speed;
          p.x += Math.sin(now * 0.001 + p.phase) * 0.55;
          if (p.y > height + 10) {
            p.y = -10;
            p.x = Math.random() * width;
          }
        }
      } else if (effect === 'stars') {
        for (const p of particles) {
          const twinkle = 0.35 + 0.65 * Math.abs(Math.sin(now * 0.0009 + p.phase));
          ctx.fillStyle = `rgba(230, 240, 255, ${p.alpha * twinkle * 0.8})`;
          ctx.beginPath();
          ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
          ctx.fill();
        }
      }
    };

    if (count > 0) {
      rafId = window.requestAnimationFrame(draw);
    }

    return () => {
      window.removeEventListener('resize', onResize);
      if (rafId) window.cancelAnimationFrame(rafId);
    };
  }, [scene.effect]);

  return (
    <div
      className="tv-weather-bg"
      style={{ background: scene.fallbackGradient }}
      aria-hidden="true"
    >
      <div
        key={sceneKey}
        className="tv-weather-photo"
        style={{ backgroundImage: `url("${scene.photoUrl}")` }}
      />
      {scene.effect === 'mist' && <div className="tv-weather-mist" />}
      {scene.effect === 'sun' && <div className="tv-weather-sunbeam" />}
      {(scene.effect === 'rain' ||
        scene.effect === 'storm' ||
        scene.effect === 'snow' ||
        scene.effect === 'stars') && <canvas ref={canvasRef} className="tv-weather-canvas" />}
      <div className="tv-weather-scrim" style={{ background: scene.scrim }} />
    </div>
  );
};
