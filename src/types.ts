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
  extension: string;
  modifiedAt: number;
}

export interface PlayerStatus {
  available: boolean;
  executable: string | null;
  source: 'configured' | 'environment' | 'bundled' | 'path' | 'unavailable';
}
