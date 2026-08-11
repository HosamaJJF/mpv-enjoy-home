export interface FolderSummary {
  id: number;
  name: string;
  path: string;
  mediaCount: number;
  addedAt: number;
  lastScannedAt: number | null;
}

export interface MediaItem {
  id: number;
  folderId: number;
  name: string;
  path: string;
  relativePath: string;
  extension: string;
  modifiedAt: number;
}

export interface LibraryEntry {
  key: string;
  name: string;
  relativePath: string;
  kind: 'folder' | 'video';
  mediaId: number | null;
  extension: string | null;
  modifiedAt: number;
  mediaCount: number;
}

export interface RecentMediaItem {
  key: string;
  sourceKind: 'local' | 'remote';
  sourceId: number;
  sourceName: string;
  targetId: string;
  targetName: string;
  name: string;
  context: string;
  itemType: string;
  updatedAt: number;
}

export interface MediaServerInput {
  kind: 'emby' | 'jellyfin';
  name: string;
  baseUrl: string;
  username: string;
  password: string;
}

export interface MediaServerCredentials {
  username: string;
  password: string;
}

export interface MediaServerSummary {
  id: number;
  kind: 'emby' | 'jellyfin';
  name: string;
  baseUrl: string;
  userId: string;
  userName: string;
  serverVersion: string | null;
  addedAt: number;
  lastConnectedAt: number | null;
}

export interface RemoteLibraryEntry {
  id: string;
  name: string;
  kind: 'collection' | 'detail';
  itemType: string;
  subtitle: string | null;
  childCount: number;
  unplayedCount: number | null;
  hasImage: boolean;
  imageAspectRatio: number | null;
  indexNumber: number | null;
  parentIndexNumber: number | null;
  updatedAt: number;
}

export interface RemoteMediaDetail {
  id: string;
  name: string;
  itemType: 'Series' | 'Movie' | 'Video' | string;
  overview: string | null;
  tagline: string | null;
  genres: string[];
  productionYear: number | null;
  premiereDate: string | null;
  runtimeTicks: number | null;
  communityRating: number | null;
  officialRating: string | null;
  played: boolean;
  playbackPositionTicks: number;
  playedPercentage: number | null;
  primaryImageId: string | null;
  backdropImageId: string | null;
  seasons: RemoteSeasonDetail[];
  episodes: RemoteEpisodeDetail[];
  people: RemotePersonDetail[];
}

export interface RemoteSeasonDetail {
  id: string;
  name: string;
  indexNumber: number | null;
  overview: string | null;
  episodeCount: number;
  unplayedCount: number | null;
  played: boolean;
  primaryImageId: string | null;
}

export interface RemoteEpisodeDetail {
  id: string;
  name: string;
  overview: string | null;
  indexNumber: number | null;
  parentIndexNumber: number | null;
  seasonId: string | null;
  premiereDate: string | null;
  runtimeTicks: number | null;
  played: boolean;
  playbackPositionTicks: number;
  playedPercentage: number | null;
  primaryImageId: string | null;
}

export interface RemotePersonDetail {
  id: string | null;
  name: string;
  role: string | null;
  personType: string | null;
  primaryImageId: string | null;
}

export interface PlayerStatus {
  available: boolean;
  executable: string | null;
  source: 'configured' | 'environment' | 'bundled' | 'path' | 'unavailable';
}

export type PlayerToggleMode = 'inherit' | 'on' | 'off';

export interface DanmakuStylePreferences {
  boldMode: PlayerToggleMode;
  fontSize: number | null;
  outline: number | null;
  shadow: number | null;
  scrollTime: number | null;
  opacity: number | null;
  displayArea: number | null;
}

export interface PlayerPreferences {
  startupVolume: number | null;
  fullscreenMode: PlayerToggleMode;
  danmakuMode: PlayerToggleMode;
  danmakuStyle: DanmakuStylePreferences;
}

export type ThemeMode = 'system' | 'light' | 'dark';
export type AccentColor = 'blue' | 'pink' | 'green' | 'yellow' | 'purple';

export interface AppearanceSettings {
  themeMode: ThemeMode;
  accentColor: AccentColor;
}
