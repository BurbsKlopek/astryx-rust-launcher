import { Catalog } from '@a2ui/web_core';
import { BASIC_FUNCTIONS } from '@a2ui/web_core/v1_0';
import {
  BadgeImpl,
  ButtonImpl,
  CardImpl,
  ColumnImpl,
  DividerImpl,
  RowImpl,
  TextImpl,
  TvAppTileImpl,
  TvClockWeatherImpl,
  TvHeroImpl,
  TvLayoutImpl,
  TvShelfImpl,
  TvStatusBarImpl,
  TvWidgetCardImpl,
} from './components';

export const ASTRYX_TV_CATALOG_ID = 'https://astryxtv.local/catalogs/tv-v1.json';

export const astryxTvCatalog = new Catalog(
  ASTRYX_TV_CATALOG_ID,
  '1.0',
  [
    TvLayoutImpl,
    TvStatusBarImpl,
    TvClockWeatherImpl,
    TvHeroImpl,
    TvShelfImpl,
    TvAppTileImpl,
    TvWidgetCardImpl,
    RowImpl,
    ColumnImpl,
    CardImpl,
    TextImpl,
    ButtonImpl,
    BadgeImpl,
    DividerImpl,
  ],
  BASIC_FUNCTIONS,
);
