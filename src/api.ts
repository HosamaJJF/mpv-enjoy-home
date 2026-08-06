import { invoke } from '@tauri-apps/api/core';
import type {
  FolderSummary,
  LibraryEntry,
  MediaItem,
  MediaServerInput,
  MediaServerSummary,
  PlayerStatus,
  RecentMediaItem,
  RemoteLibraryEntry,
  RemoteMediaDetail,
} from './types';

export const api = {
  listFolders: () => invoke<FolderSummary[]>('list_library_folders'),
  addFolder: (path: string) =>
    invoke<FolderSummary>('add_library_folder', { path }),
  rescanFolder: (folderId: number) =>
    invoke<FolderSummary>('rescan_library_folder', { folderId }),
  removeFolder: (folderId: number) =>
    invoke<void>('remove_library_folder', { folderId }),
  listMedia: (folderId?: number, query?: string, limit = 500) =>
    invoke<MediaItem[]>('list_media', {
      folderId: folderId ?? null,
      query: query || null,
      limit,
    }),
  listLibraryEntries: (folderId: number, parent?: string, query?: string) =>
    invoke<LibraryEntry[]>('list_library_entries', {
      folderId,
      parent: parent || null,
      query: query || null,
    }),
  listRecentMedia: (limit = 8) =>
    invoke<RecentMediaItem[]>('list_recent_media', { limit }),
  listMediaServers: () => invoke<MediaServerSummary[]>('list_media_servers'),
  addMediaServer: (input: MediaServerInput) =>
    invoke<MediaServerSummary>('add_media_server', { input }),
  removeMediaServer: (serverId: number) =>
    invoke<void>('remove_media_server', { serverId }),
  listRemoteEntries: (serverId: number, parentId?: string) =>
    invoke<RemoteLibraryEntry[]>('list_remote_entries', {
      serverId,
      parentId: parentId || null,
    }),
  getRemoteImage: (
    serverId: number,
    itemId: string,
    imageType: 'Primary' | 'Backdrop' = 'Primary',
    maxWidth = 360,
  ) =>
    invoke<string | null>('get_remote_image', {
      serverId,
      itemId,
      imageType,
      maxWidth,
    }),
  getRemoteMediaDetail: (serverId: number, itemId: string) =>
    invoke<RemoteMediaDetail>('get_remote_media_detail', { serverId, itemId }),
  getPlayerStatus: () => invoke<PlayerStatus>('get_player_status'),
  setPlayerExecutable: (path?: string) =>
    invoke<PlayerStatus>('set_player_executable', { path: path ?? null }),
  playMedia: (mediaId: number) => invoke<void>('play_media', { mediaId }),
  playRemoteMedia: (serverId: number, itemId: string) =>
    invoke<void>('play_remote_media', { serverId, itemId }),
};
