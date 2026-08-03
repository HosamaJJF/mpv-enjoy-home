import { invoke } from '@tauri-apps/api/core';
import type { FolderSummary, MediaItem, PlayerStatus } from './types';

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
  getPlayerStatus: () => invoke<PlayerStatus>('get_player_status'),
  setPlayerExecutable: (path?: string) =>
    invoke<PlayerStatus>('set_player_executable', { path: path ?? null }),
  playMedia: (mediaId: number) => invoke<void>('play_media', { mediaId }),
};
