import { z } from 'zod';
import { CommonSchemas } from '@a2ui/web_core/v1_0';

export const TvLayoutApi = {
  name: 'TvLayout',
  schema: z.object({
    children: CommonSchemas.ChildList,
  }),
};

export const TvStatusBarApi = {
  name: 'TvStatusBar',
  schema: z.object({
    brandTitle: CommonSchemas.DynamicString,
    timeText: CommonSchemas.DynamicString.optional(),
    dateText: CommonSchemas.DynamicString.optional(),
    weatherText: CommonSchemas.DynamicString.optional(),
    networkType: CommonSchemas.DynamicString.optional(),
    isConnected: CommonSchemas.DynamicBoolean.optional(),
    isVpnActive: CommonSchemas.DynamicBoolean.optional(),
    deviceLabel: CommonSchemas.DynamicString.optional(),
    onOpenSearch: CommonSchemas.Action.optional(),
    onOpenNetwork: CommonSchemas.Action.optional(),
    onOpenVpn: CommonSchemas.Action.optional(),
    onOpenTheme: CommonSchemas.Action.optional(),
    onOpenStudio: CommonSchemas.Action.optional(),
    onOpenSettings: CommonSchemas.Action.optional(),
    onRefresh: CommonSchemas.Action.optional(),
  }),
};

export const TvClockWeatherApi = {
  name: 'TvClockWeather',
  schema: z.object({
    timeMain: CommonSchemas.DynamicString,
    timeSeconds: CommonSchemas.DynamicString,
    timePeriod: CommonSchemas.DynamicString.optional(),
    clock24h: CommonSchemas.DynamicBoolean.optional(),
    fullDate: CommonSchemas.DynamicString,
    locationLabel: CommonSchemas.DynamicString,
    showWeather: CommonSchemas.DynamicBoolean.optional(),
    tempText: CommonSchemas.DynamicString,
    conditionText: CommonSchemas.DynamicString,
    iconKey: CommonSchemas.DynamicString.optional(),
    feelsLikeText: CommonSchemas.DynamicString.optional(),
    highLowText: CommonSchemas.DynamicString.optional(),
    humidityText: CommonSchemas.DynamicString.optional(),
    windText: CommonSchemas.DynamicString.optional(),
    sunText: CommonSchemas.DynamicString.optional(),
    updatedText: CommonSchemas.DynamicString.optional(),
    onOpenWeather: CommonSchemas.Action.optional(),
  }),
};

export const TvHeroApi = {
  name: 'TvHero',
  schema: z.object({
    eyebrow: CommonSchemas.DynamicString.optional(),
    title: CommonSchemas.DynamicString,
    subtitle: CommonSchemas.DynamicString.optional(),
    badgeText: CommonSchemas.DynamicString.optional(),
    primaryLabel: CommonSchemas.DynamicString.optional(),
    secondaryLabel: CommonSchemas.DynamicString.optional(),
    onPrimary: CommonSchemas.Action.optional(),
    onSecondary: CommonSchemas.Action.optional(),
  }),
};

export const TvShelfApi = {
  name: 'TvShelf',
  schema: z.object({
    title: CommonSchemas.DynamicString,
    countText: CommonSchemas.DynamicString.optional(),
    layout: CommonSchemas.DynamicString.optional(),
    children: CommonSchemas.ChildList,
  }),
};

export const TvAppTileApi = {
  name: 'TvAppTile',
  schema: z.object({
    packageName: CommonSchemas.DynamicString,
    label: CommonSchemas.DynamicString,
    category: CommonSchemas.DynamicString.optional(),
    bannerUrl: CommonSchemas.DynamicString,
    iconUrl: CommonSchemas.DynamicString.optional(),
    hasBanner: CommonSchemas.DynamicBoolean.optional(),
    isPinned: CommonSchemas.DynamicBoolean.optional(),
    showLabel: CommonSchemas.DynamicBoolean.optional(),
    onLaunch: CommonSchemas.Action,
    onContextMenu: CommonSchemas.Action.optional(),
  }),
};

export const TvWidgetCardApi = {
  name: 'TvWidgetCard',
  schema: z.object({
    title: CommonSchemas.DynamicString,
    value: CommonSchemas.DynamicString,
    subtitle: CommonSchemas.DynamicString.optional(),
    badge: CommonSchemas.DynamicString.optional(),
    icon: CommonSchemas.DynamicString.optional(),
    accent: CommonSchemas.DynamicString.optional(),
    onAction: CommonSchemas.Action.optional(),
  }),
};

// Standard A2UI-compatible primitives backed by Astryx Core
export const RowApi = {
  name: 'Row',
  schema: z.object({
    children: CommonSchemas.ChildList,
    gap: CommonSchemas.DynamicNumber.optional(),
  }),
};

export const ColumnApi = {
  name: 'Column',
  schema: z.object({
    children: CommonSchemas.ChildList,
    gap: CommonSchemas.DynamicNumber.optional(),
  }),
};

export const CardApi = {
  name: 'Card',
  schema: z.object({
    child: CommonSchemas.ComponentId.optional(),
    children: CommonSchemas.ChildList.optional(),
  }),
};

export const TextApi = {
  name: 'Text',
  schema: z.object({
    text: CommonSchemas.DynamicString,
    usageHint: CommonSchemas.DynamicString.optional(),
  }),
};

export const ButtonApi = {
  name: 'Button',
  schema: z.object({
    label: CommonSchemas.DynamicString,
    variant: CommonSchemas.DynamicString.optional(),
    action: CommonSchemas.Action,
  }),
};

export const BadgeApi = {
  name: 'Badge',
  schema: z.object({
    label: CommonSchemas.DynamicString,
    variant: CommonSchemas.DynamicString.optional(),
  }),
};

export const DividerApi = {
  name: 'Divider',
  schema: z.object({}),
};
