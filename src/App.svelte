<script lang="ts">
  import { getVersion } from '@tauri-apps/api/app';
  import { open } from '@tauri-apps/plugin-dialog';
  import { onMount } from 'svelte';
  import {
    api,
    invokeErrorMessage,
    isRemoteAuthenticationRequired,
  } from './api';
  import Icon from './components/Icon.svelte';
  import PreferenceRange from './components/PreferenceRange.svelte';
  import type {
    AccentColor,
    AppInstallType,
    AppearanceSettings,
    DanmakuStylePreferences,
    FolderSummary,
    LibraryEntry,
    MediaServerInput,
    MediaServerSummary,
    PlayerPreferences,
    PlayerStatus,
    PlayerToggleMode,
    RecentMediaItem,
    RemoteEpisodeDetail,
    RemoteLibraryEntry,
    RemoteMediaDetail,
    RemoteSeasonDetail,
    ThemeMode,
    UpdateCheckResult,
  } from './types';

  type View = 'home' | 'library' | 'settings';
  type Toast = { kind: 'success' | 'error'; message: string };
  type LibrarySort = 'name-asc' | 'name-desc' | 'time-desc' | 'time-asc';
  type LibrarySource =
    | { kind: 'local'; folder: FolderSummary }
    | { kind: 'remote'; server: MediaServerSummary };
  type RemoteCrumb = {
    id: string | null;
    name: string;
    mode: 'list' | 'detail';
  };

  const themeOptions: { value: ThemeMode; label: string }[] = [
    { value: 'system', label: '跟随系统' },
    { value: 'light', label: '明亮' },
    { value: 'dark', label: '黑暗' },
  ];
  const accentOptions: { value: AccentColor; label: string }[] = [
    { value: 'blue', label: '蓝色' },
    { value: 'pink', label: '粉色' },
    { value: 'green', label: '绿色' },
    { value: 'yellow', label: '黄色' },
    { value: 'purple', label: '紫色' },
  ];
  const playerToggleOptions: { value: PlayerToggleMode; label: string }[] = [
    { value: 'inherit', label: '跟随 mpv' },
    { value: 'on', label: '开启' },
    { value: 'off', label: '关闭' },
  ];
  const danmakuToggleOptions: { value: PlayerToggleMode; label: string }[] = [
    { value: 'inherit', label: '跟随插件' },
    { value: 'on', label: '开启' },
    { value: 'off', label: '关闭' },
  ];
  const librarySortOptions: { value: LibrarySort; label: string }[] = [
    { value: 'name-asc', label: '名称：升序' },
    { value: 'name-desc', label: '名称：降序' },
    { value: 'time-desc', label: '时间：新到旧' },
    { value: 'time-asc', label: '时间：旧到新' },
  ];
  const libraryNameCollator = new Intl.Collator('zh-CN', {
    numeric: true,
    sensitivity: 'base',
  });
  const defaultDanmakuStyle: DanmakuStylePreferences = {
    boldMode: 'inherit',
    fontSize: null,
    outline: null,
    shadow: null,
    scrollTime: null,
    opacity: null,
    displayArea: null,
  };

  let view = $state<View>('home');
  let folders = $state<FolderSummary[]>([]);
  let servers = $state<MediaServerSummary[]>([]);
  let recentMedia = $state<RecentMediaItem[]>([]);
  let selectedSource = $state<LibrarySource | null>(null);
  let localEntries = $state<LibraryEntry[]>([]);
  let remoteEntries = $state<RemoteLibraryEntry[]>([]);
  let remoteImages = $state<Record<string, string>>({});
  let remoteBackdrop = $state<string | null>(null);
  let remoteDetail = $state<RemoteMediaDetail | null>(null);
  let selectedSeasonId = $state<string | null>(null);
  let currentPath = $state('');
  let remoteCrumbs = $state<RemoteCrumb[]>([]);
  let player = $state<PlayerStatus | null>(null);
  let playerPreferences = $state<PlayerPreferences>({
    startupVolume: null,
    fullscreenMode: 'inherit',
    danmakuMode: 'inherit',
    danmakuStyle: defaultDanmakuStyle,
  });
  let volumeDraft = $state(70);
  let danmakuFontSizeDraft = $state(50);
  let danmakuOutlineDraft = $state(1);
  let danmakuShadowDraft = $state(0);
  let danmakuScrollTimeDraft = $state(15);
  let danmakuOpacityDraft = $state(0.7);
  let danmakuDisplayAreaDraft = $state(0.85);
  let search = $state('');
  let librarySort = $state<LibrarySort>('name-asc');
  let busy = $state(false);
  let libraryLoading = $state(false);
  let busyMessage = $state('正在整理媒体库…');
  let toast = $state<Toast | null>(null);
  let showServerForm = $state(false);
  let reauthenticatingServer = $state<MediaServerSummary | null>(null);
  let resumeServerAfterLogin = $state(false);
  let sidebarCollapsed = $state(false);
  let appearance = $state<AppearanceSettings>({
    themeMode: 'system',
    accentColor: 'blue',
  });
  let systemPrefersDark = $state(false);
  let appearanceSaving = $state(false);
  let playerPreferencesSaving = $state(false);
  let appVersion = $state<string | null>(null);
  let updateCheckResult = $state<UpdateCheckResult | null>(null);
  let checkingUpdate = $state(false);
  let updating = $state(false);
  let serverDraft = $state<MediaServerInput>(emptyServerDraft());
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let localLoadGeneration = 0;
  let remoteLoadGeneration = 0;

  const totalMedia = $derived(
    folders.reduce((total, folder) => total + folder.mediaCount, 0),
  );
  const sourceCount = $derived(folders.length + servers.length);
  const visibleLocalEntries = $derived(
    sortEntries(
      filterEntries(localEntries, search, (entry) =>
        `${entry.name} ${entry.relativePath}`.toLocaleLowerCase(),
      ),
      (entry) => entry.modifiedAt,
    ),
  );
  const localDirectories = $derived(
    visibleLocalEntries.filter((entry) => entry.kind === 'folder'),
  );
  const localVideos = $derived(
    visibleLocalEntries.filter((entry) => entry.kind === 'video'),
  );
  const visibleRemoteEntries = $derived(
    sortEntries(
      filterEntries(remoteEntries, search, (entry) =>
        `${entry.name} ${entry.subtitle ?? ''}`.toLocaleLowerCase(),
      ),
      (entry) => entry.updatedAt,
    ),
  );
  const selectedSeason = $derived(
    remoteDetail?.seasons.find((season) => season.id === selectedSeasonId) ??
      remoteDetail?.seasons[0] ??
      null,
  );
  const selectedEpisodes = $derived.by(() => {
    if (!remoteDetail) return [];
    if (!selectedSeason) return remoteDetail.episodes;
    return remoteDetail.episodes.filter(
      (episode) =>
        episode.seasonId === selectedSeason.id ||
        (selectedSeason.indexNumber !== null &&
          episode.parentIndexNumber === selectedSeason.indexNumber),
    );
  });
  const effectiveTheme = $derived(
    appearance.themeMode === 'system'
      ? systemPrefersDark
        ? 'dark'
        : 'light'
      : appearance.themeMode,
  );

  $effect(() => {
    document.documentElement.dataset.theme = effectiveTheme;
    document.documentElement.dataset.accent = appearance.accentColor;
    document.documentElement.style.colorScheme = effectiveTheme;
  });

  onMount(() => {
    const systemTheme = window.matchMedia('(prefers-color-scheme: dark)');
    const syncSystemTheme = () => (systemPrefersDark = systemTheme.matches);
    syncSystemTheme();
    systemTheme.addEventListener('change', syncSystemTheme);
    void initializeApp();
    void loadAppVersion();
    return () => systemTheme.removeEventListener('change', syncSystemTheme);
  });

  async function loadAppVersion() {
    try {
      appVersion = await getVersion();
    } catch {
      appVersion = null;
    }
    void checkUpdate(true);
  }

  async function checkUpdate(silent = false) {
    if (checkingUpdate || updating) return;
    checkingUpdate = true;
    try {
      const result = await api.checkAppUpdate();
      updateCheckResult = result;
      if (!silent) {
        if (result.hasUpdate) {
          notify('success', `发现新版本 v${result.latestVersion}`);
        } else {
          notify('success', `当前已是最新版本 (v${result.currentVersion})`);
        }
      }
    } catch (error) {
      if (!silent) {
        notify('error', `检查更新失败：${normalizeError(error)}`);
      }
    } finally {
      checkingUpdate = false;
    }
  }

  async function applyUpdate() {
    if (!updateCheckResult?.matchedAsset || updating) return;
    const asset = updateCheckResult.matchedAsset;
    updating = true;
    try {
      const result = await api.downloadAndApplyUpdate(
        asset.downloadUrl,
        asset.name,
      );
      notify('success', result.message);
    } catch (error) {
      notify('error', `更新失败：${normalizeError(error)}`);
    } finally {
      updating = false;
    }
  }

  function openExternal(url: string) {
    void api.openExternalUrl(url);
  }

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
  }

  function installTypeLabel(type: AppInstallType): string {
    switch (type) {
      case 'mac-app':
        return 'macOS';
      case 'windows-setup':
        return 'Windows 安装版';
      case 'windows-portable':
        return 'Windows 免安装版';
    }
  }

  async function initializeApp() {
    try {
      appearance = await api.getAppearanceSettings();
    } catch (error) {
      notify('error', normalizeError(error));
    }
    try {
      playerPreferences = await api.getPlayerPreferences();
      volumeDraft = playerPreferences.startupVolume ?? 70;
      syncDanmakuStyleDrafts(playerPreferences.danmakuStyle, true);
    } catch (error) {
      notify('error', normalizeError(error));
    }
    await refreshOverview();
  }

  async function updateAppearance(next: AppearanceSettings) {
    if (appearanceSaving) return;
    const previous = appearance;
    appearance = next;
    appearanceSaving = true;
    try {
      appearance = await api.setAppearanceSettings(next);
    } catch (error) {
      appearance = previous;
      notify('error', normalizeError(error));
    } finally {
      appearanceSaving = false;
    }
  }

  async function updatePlayerPreferences(next: PlayerPreferences) {
    if (playerPreferencesSaving) return;
    const previous = playerPreferences;
    playerPreferences = next;
    playerPreferencesSaving = true;
    try {
      playerPreferences = await api.setPlayerPreferences(next);
      volumeDraft = playerPreferences.startupVolume ?? volumeDraft;
      syncDanmakuStyleDrafts(playerPreferences.danmakuStyle);
    } catch (error) {
      playerPreferences = previous;
      volumeDraft = previous.startupVolume ?? volumeDraft;
      syncDanmakuStyleDrafts(previous.danmakuStyle);
      notify('error', normalizeError(error));
    } finally {
      playerPreferencesSaving = false;
    }
  }

  function updateDanmakuStyle(patch: Partial<DanmakuStylePreferences>) {
    return updatePlayerPreferences({
      ...playerPreferences,
      danmakuStyle: {
        ...playerPreferences.danmakuStyle,
        ...patch,
      },
    });
  }

  function syncDanmakuStyleDrafts(
    style: DanmakuStylePreferences,
    resetMissing = false,
  ) {
    danmakuFontSizeDraft =
      style.fontSize ?? (resetMissing ? 50 : danmakuFontSizeDraft);
    danmakuOutlineDraft =
      style.outline ?? (resetMissing ? 1 : danmakuOutlineDraft);
    danmakuShadowDraft =
      style.shadow ?? (resetMissing ? 0 : danmakuShadowDraft);
    danmakuScrollTimeDraft =
      style.scrollTime ?? (resetMissing ? 15 : danmakuScrollTimeDraft);
    danmakuOpacityDraft =
      style.opacity ?? (resetMissing ? 0.7 : danmakuOpacityDraft);
    danmakuDisplayAreaDraft =
      style.displayArea ?? (resetMissing ? 0.85 : danmakuDisplayAreaDraft);
  }

  async function refreshOverview() {
    try {
      [folders, servers, player] = await Promise.all([
        api.listFolders(),
        api.listMediaServers(),
        api.getPlayerStatus(),
      ]);
      if (selectedSource?.kind === 'local') {
        const selectedFolderId = selectedSource.folder.id;
        const folder = folders.find((entry) => entry.id === selectedFolderId);
        selectedSource = folder ? { kind: 'local', folder } : null;
      } else if (selectedSource?.kind === 'remote') {
        const selectedServerId = selectedSource.server.id;
        const server = servers.find((entry) => entry.id === selectedServerId);
        selectedSource = server ? { kind: 'remote', server } : null;
      }
      recentMedia = await api.listRecentMedia(8);
    } catch (error) {
      notify('error', normalizeError(error));
    }
  }

  async function addFolder() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: '选择媒体目录',
    });
    if (typeof selected !== 'string') return;

    await withBusy('正在扫描新媒体库…', async () => {
      const folder = await api.addFolder(selected);
      await refreshOverview();
      notify(
        'success',
        `已添加“${folder.name}”，找到 ${folder.mediaCount} 个视频`,
      );
    });
  }

  function showConnectServer() {
    view = 'settings';
    serverDraft = emptyServerDraft();
    reauthenticatingServer = null;
    resumeServerAfterLogin = false;
    showServerForm = true;
  }

  function showServerReauthentication(
    server: MediaServerSummary,
    resumeLibrary = false,
  ) {
    if (reauthenticatingServer?.id === server.id && showServerForm) {
      resumeServerAfterLogin ||= resumeLibrary;
      view = 'settings';
      return;
    }
    view = 'settings';
    reauthenticatingServer = server;
    resumeServerAfterLogin = resumeLibrary;
    serverDraft = {
      kind: server.kind,
      name: server.name,
      baseUrl: server.baseUrl,
      username: server.userName,
      password: '',
    };
    showServerForm = true;
  }

  function closeServerForm() {
    serverDraft = emptyServerDraft();
    reauthenticatingServer = null;
    resumeServerAfterLogin = false;
    showServerForm = false;
  }

  async function connectServer() {
    const reconnecting = reauthenticatingServer;
    const resumeLibrary = resumeServerAfterLogin;
    await withBusy(
      reconnecting
        ? `正在重新登录“${reconnecting.name}”…`
        : `正在连接 ${serverKindLabel(serverDraft.kind)}…`,
      async () => {
        const server = reconnecting
          ? await api.reauthenticateMediaServer(reconnecting.id, {
              username: serverDraft.username.trim(),
              password: serverDraft.password,
            })
          : await api.addMediaServer({
              kind: serverDraft.kind,
              name: serverDraft.name.trim(),
              baseUrl: serverDraft.baseUrl.trim(),
              username: serverDraft.username.trim(),
              password: serverDraft.password,
            });
        closeServerForm();
        await refreshOverview();
        notify(
          'success',
          reconnecting
            ? `“${server.name}”已重新登录`
            : `已连接“${server.name}”`,
        );
        if (reconnecting && resumeLibrary) await openRemoteServer(server);
      },
    );
  }

  async function openLocalFolder(
    folder: FolderSummary,
    path = '',
    refreshIndex = true,
  ) {
    const generation = ++localLoadGeneration;
    remoteLoadGeneration += 1;
    view = 'library';
    selectedSource = { kind: 'local', folder };
    currentPath = path;
    remoteCrumbs = [];
    remoteEntries = [];
    remoteImages = {};
    remoteBackdrop = null;
    remoteDetail = null;
    search = '';
    libraryLoading = true;
    let activeFolder = folder;
    let indexUpdated = false;
    try {
      if (refreshIndex) {
        try {
          activeFolder = await api.rescanFolder(folder.id);
          if (generation !== localLoadGeneration) return;
          indexUpdated = true;
          selectedSource = { kind: 'local', folder: activeFolder };
          folders = folders.map((entry) =>
            entry.id === activeFolder.id ? activeFolder : entry,
          );
        } catch (error) {
          if (generation !== localLoadGeneration) return;
          notify(
            'error',
            `自动扫描失败，正在显示上次索引：${normalizeError(error)}`,
          );
        }
      }
      const entries = await api.listLibraryEntries(activeFolder.id, path);
      if (generation !== localLoadGeneration) return;
      localEntries = entries;
    } catch (error) {
      if (generation === localLoadGeneration) {
        notify('error', normalizeError(error));
      }
    } finally {
      if (generation === localLoadGeneration) libraryLoading = false;
    }
    if (indexUpdated && generation === localLoadGeneration) {
      void refreshOverview();
    }
  }

  async function openRemoteServer(server: MediaServerSummary) {
    localLoadGeneration += 1;
    view = 'library';
    selectedSource = { kind: 'remote', server };
    currentPath = '';
    localEntries = [];
    search = '';
    remoteCrumbs = [{ id: null, name: server.name, mode: 'list' }];
    await loadRemoteEntries(server, null);
  }

  async function openRemoteCollection(entry: RemoteLibraryEntry) {
    if (selectedSource?.kind !== 'remote') return;
    remoteCrumbs = [
      ...remoteCrumbs,
      { id: entry.id, name: entry.name, mode: 'list' },
    ];
    search = '';
    await loadRemoteEntries(selectedSource.server, entry.id);
  }

  async function openRemoteDetail(entry: RemoteLibraryEntry) {
    if (selectedSource?.kind !== 'remote') return;
    remoteCrumbs = [
      ...remoteCrumbs,
      { id: entry.id, name: entry.name, mode: 'detail' },
    ];
    search = '';
    await loadRemoteDetail(selectedSource.server, entry.id);
  }

  async function navigateRemoteCrumb(index: number) {
    if (selectedSource?.kind !== 'remote') return;
    remoteCrumbs = remoteCrumbs.slice(0, index + 1);
    search = '';
    const crumb = remoteCrumbs[index];
    if (crumb?.mode === 'detail' && crumb.id) {
      await loadRemoteDetail(selectedSource.server, crumb.id);
    } else {
      await loadRemoteEntries(selectedSource.server, crumb?.id ?? null);
    }
  }

  async function loadRemoteEntries(
    server: MediaServerSummary,
    parentId: string | null,
  ) {
    const generation = ++remoteLoadGeneration;
    libraryLoading = true;
    remoteImages = {};
    remoteBackdrop = null;
    remoteDetail = null;
    selectedSeasonId = null;
    try {
      const entries = await api.listRemoteEntries(
        server.id,
        parentId ?? undefined,
      );
      if (generation !== remoteLoadGeneration) return;
      remoteEntries = entries;
      void loadRemoteImages(server.id, entries, generation);
    } catch (error) {
      handleRemoteError(error, server);
    } finally {
      if (generation === remoteLoadGeneration) libraryLoading = false;
    }
  }

  async function loadRemoteDetail(server: MediaServerSummary, itemId: string) {
    const generation = ++remoteLoadGeneration;
    libraryLoading = true;
    remoteEntries = [];
    remoteImages = {};
    remoteBackdrop = null;
    try {
      const detail = await api.getRemoteMediaDetail(server.id, itemId);
      if (generation !== remoteLoadGeneration) return;
      remoteDetail = detail;
      selectedSeasonId = preferredSeason(detail)?.id ?? null;
      void loadRemoteDetailImages(server.id, detail, generation);
    } catch (error) {
      handleRemoteError(error, server);
    } finally {
      if (generation === remoteLoadGeneration) libraryLoading = false;
    }
  }

  async function loadRemoteDetailImages(
    serverId: number,
    detail: RemoteMediaDetail,
    generation: number,
  ) {
    const backdropPromise = detail.backdropImageId
      ? loadRemoteBackdrop(serverId, detail.backdropImageId, generation)
      : Promise.resolve();

    const initialSeason = preferredSeason(detail);
    const initialEpisodes = initialSeason
      ? detail.episodes.filter((episode) =>
          episodeBelongsToSeason(episode, initialSeason),
        )
      : detail.episodes;
    const remainingEpisodes = detail.episodes.filter(
      (episode) => !initialEpisodes.includes(episode),
    );
    const imageIds = [
      detail.primaryImageId,
      ...detail.seasons.map((season) => season.primaryImageId),
      ...initialEpisodes.map((episode) => episode.primaryImageId),
      ...detail.people.slice(0, 30).map((person) => person.primaryImageId),
      ...remainingEpisodes.map((episode) => episode.primaryImageId),
    ].filter((value): value is string => !!value);
    const uniqueIds = [...new Set(imageIds)].slice(0, 120);
    await loadRemoteImageSet(serverId, uniqueIds, generation);
    await backdropPromise;
  }

  async function selectRemoteSeason(seasonId: string) {
    selectedSeasonId = seasonId;
    if (!remoteDetail || selectedSource?.kind !== 'remote') return;
    const serverId = selectedSource.server.id;
    const season = remoteDetail.seasons.find((entry) => entry.id === seasonId);
    if (!season) return;
    const imageIds = [
      season.primaryImageId,
      ...remoteDetail.episodes
        .filter((episode) => episodeBelongsToSeason(episode, season))
        .map((episode) => episode.primaryImageId),
    ]
      .filter((value): value is string => !!value && !remoteImages[value])
      .slice(0, 120);
    const generation = remoteLoadGeneration;
    await loadRemoteImageSet(serverId, imageIds, generation);
  }

  async function loadRemoteImages(
    serverId: number,
    entries: RemoteLibraryEntry[],
    generation: number,
  ) {
    const targets = sortEntries(entries, (entry) => entry.updatedAt)
      .filter((entry) => entry.hasImage)
      .slice(0, 40);
    await loadRemoteImageSet(
      serverId,
      targets.map((entry) => entry.id),
      generation,
    );
  }

  function updateLibrarySort(value: string) {
    if (!librarySortOptions.some((option) => option.value === value)) return;
    librarySort = value as LibrarySort;
    if (selectedSource?.kind !== 'remote' || remoteDetail) return;

    const imageIds = sortEntries(remoteEntries, (entry) => entry.updatedAt)
      .filter((entry) => entry.hasImage && !remoteImages[entry.id])
      .map((entry) => entry.id)
      .slice(0, 40);
    void loadRemoteImageSet(
      selectedSource.server.id,
      imageIds,
      remoteLoadGeneration,
    );
  }

  async function loadRemoteBackdrop(
    serverId: number,
    itemId: string,
    generation: number,
  ) {
    try {
      const backdrop = await api.getRemoteImage(
        serverId,
        itemId,
        'Backdrop',
        1600,
      );
      if (generation === remoteLoadGeneration) remoteBackdrop = backdrop;
    } catch (error) {
      const server = servers.find((entry) => entry.id === serverId);
      if (server && handleRemoteAuthenticationError(error, server)) return;
      // The detail remains useful when a server image has disappeared.
    }
  }

  async function loadRemoteImageSet(
    serverId: number,
    imageIds: string[],
    generation: number,
  ) {
    let nextIndex = 0;
    const loadNext = async () => {
      while (generation === remoteLoadGeneration) {
        const imageId = imageIds[nextIndex];
        nextIndex += 1;
        if (imageId === undefined) return;

        try {
          const image = await api.getRemoteImage(serverId, imageId);
          if (image && generation === remoteLoadGeneration) {
            remoteImages = { ...remoteImages, [imageId]: image };
          }
        } catch (error) {
          const server = servers.find((entry) => entry.id === serverId);
          if (server && handleRemoteAuthenticationError(error, server)) return;
          // Missing remote artwork uses the CSS fallback.
        }
      }
    };

    await Promise.all(
      Array.from({ length: Math.min(6, imageIds.length) }, loadNext),
    );
  }

  async function rescanFolder(folder: FolderSummary) {
    await withBusy(`正在重新扫描“${folder.name}”…`, async () => {
      const updated = await api.rescanFolder(folder.id);
      await refreshOverview();
      if (
        selectedSource?.kind === 'local' &&
        selectedSource.folder.id === folder.id
      ) {
        selectedSource = { kind: 'local', folder: updated };
        localEntries = await api.listLibraryEntries(updated.id, currentPath);
      }
      notify('success', `扫描完成，共 ${updated.mediaCount} 个视频`);
    });
  }

  async function removeFolder(folder: FolderSummary) {
    if (!window.confirm(`从首页移除“${folder.name}”？媒体文件不会被删除。`)) {
      return;
    }
    try {
      await api.removeFolder(folder.id);
      if (
        selectedSource?.kind === 'local' &&
        selectedSource.folder.id === folder.id
      ) {
        selectedSource = null;
        localEntries = [];
        view = 'home';
      }
      await refreshOverview();
      notify('success', '已移除媒体目录，原文件没有变化');
    } catch (error) {
      notify('error', normalizeError(error));
    }
  }

  async function removeServer(server: MediaServerSummary) {
    if (!window.confirm(`移除“${server.name}”连接？服务器内容不会受到影响。`)) {
      return;
    }
    try {
      await api.removeMediaServer(server.id);
      if (reauthenticatingServer?.id === server.id) closeServerForm();
      if (
        selectedSource?.kind === 'remote' &&
        selectedSource.server.id === server.id
      ) {
        selectedSource = null;
        remoteEntries = [];
        remoteDetail = null;
        remoteBackdrop = null;
        view = 'home';
      }
      await refreshOverview();
      notify('success', '已移除媒体服务器连接');
    } catch (error) {
      notify('error', normalizeError(error));
    }
  }

  async function playLocal(entry: LibraryEntry) {
    if (entry.mediaId === null) return;
    try {
      await api.playMedia(entry.mediaId);
      notify('success', `已交给 mpv 播放：${entry.name}`);
    } catch (error) {
      handlePlaybackError(error);
    }
  }

  async function playRemoteItem(itemId: string, name: string) {
    if (selectedSource?.kind !== 'remote') return;
    const server = selectedSource.server;
    try {
      await api.playRemoteMedia(server.id, itemId);
      notify('success', `已交给 mpv 播放：${name}`);
    } catch (error) {
      if (handleRemoteAuthenticationError(error, server)) return;
      handlePlaybackError(error);
    }
  }

  function handlePlaybackError(error: unknown) {
    notify('error', normalizeError(error));
    if (!player?.available) view = 'settings';
  }

  async function choosePlayer() {
    const selected = await open({
      directory: false,
      multiple: false,
      title: '选择 mpv 可执行文件或 macOS App',
    });
    if (typeof selected !== 'string') return;
    try {
      player = await api.setPlayerExecutable(selected);
      notify('success', '播放器设置已保存');
    } catch (error) {
      notify('error', normalizeError(error));
    }
  }

  async function resetPlayer() {
    try {
      player = await api.setPlayerExecutable();
      notify('success', '已恢复自动发现播放器');
    } catch (error) {
      notify('error', normalizeError(error));
    }
  }

  function openLibrarySources() {
    localLoadGeneration += 1;
    remoteLoadGeneration += 1;
    libraryLoading = false;
    view = 'library';
    selectedSource = null;
    localEntries = [];
    remoteEntries = [];
    remoteDetail = null;
    remoteImages = {};
    remoteBackdrop = null;
    remoteCrumbs = [];
    currentPath = '';
    search = '';
  }

  function openRecent(item: RecentMediaItem) {
    if (item.sourceKind === 'local') {
      const folder = folders.find((entry) => entry.id === item.sourceId);
      if (folder) void openLocalFolder(folder, item.targetId);
      return;
    }

    const server = servers.find((entry) => entry.id === item.sourceId);
    if (!server) return;
    localLoadGeneration += 1;
    view = 'library';
    selectedSource = { kind: 'remote', server };
    currentPath = '';
    localEntries = [];
    search = '';
    remoteCrumbs = [
      { id: null, name: server.name, mode: 'list' },
      { id: item.targetId, name: item.targetName, mode: 'detail' },
    ];
    void loadRemoteDetail(server, item.targetId);
  }

  function navigateLocal(path: string) {
    if (selectedSource?.kind === 'local') {
      void openLocalFolder(selectedSource.folder, path, false);
    }
  }

  function rescanSelectedFolder() {
    if (selectedSource?.kind === 'local') {
      void rescanFolder(selectedSource.folder);
    }
  }

  function localBreadcrumbs() {
    if (selectedSource?.kind !== 'local') return [];
    const crumbs = [{ name: selectedSource.folder.name, path: '' }];
    let path = '';
    for (const segment of currentPath.split('/').filter(Boolean)) {
      path = path ? `${path}/${segment}` : segment;
      crumbs.push({ name: segment, path });
    }
    return crumbs;
  }

  async function withBusy(message: string, action: () => Promise<void>) {
    busyMessage = message;
    busy = true;
    try {
      await action();
    } catch (error) {
      notify('error', normalizeError(error));
    } finally {
      busy = false;
    }
  }

  function notify(kind: Toast['kind'], message: string) {
    if (toastTimer) clearTimeout(toastTimer);
    toast = { kind, message };
    toastTimer = setTimeout(() => (toast = null), 3600);
  }

  function handleRemoteAuthenticationError(
    error: unknown,
    server: MediaServerSummary,
  ) {
    if (!isRemoteAuthenticationRequired(error)) return false;
    const alreadyPrompting =
      reauthenticatingServer?.id === server.id && showServerForm;
    const resumeLibrary =
      view === 'library' &&
      selectedSource?.kind === 'remote' &&
      selectedSource.server.id === server.id;
    showServerReauthentication(server, resumeLibrary);
    if (!alreadyPrompting) {
      notify('error', `“${server.name}”登录已失效，请重新登录`);
    }
    return true;
  }

  function handleRemoteError(error: unknown, server: MediaServerSummary) {
    if (!handleRemoteAuthenticationError(error, server)) {
      notify('error', normalizeError(error));
    }
  }

  function normalizeError(error: unknown) {
    return invokeErrorMessage(error);
  }

  function formatScanTime(timestamp: number | null) {
    if (!timestamp) return '尚未扫描';
    return new Intl.DateTimeFormat('zh-CN', {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    }).format(new Date(timestamp * 1000));
  }

  function formatUpdateTime(timestamp: number) {
    if (timestamp <= 0) return '最近更新';
    return new Intl.DateTimeFormat('zh-CN', {
      month: 'short',
      day: 'numeric',
    }).format(new Date(timestamp * 1000));
  }

  function sourceLabel(source: PlayerStatus['source']) {
    return {
      configured: '手动选择',
      environment: '环境变量',
      bundled: '随应用提供',
      path: '系统 PATH',
      unavailable: '未发现',
    }[source];
  }

  function serverKindLabel(kind: MediaServerSummary['kind']) {
    return kind === 'emby' ? 'Emby' : 'Jellyfin';
  }

  function remoteTypeLabel(entry: RemoteLibraryEntry) {
    return remoteItemTypeLabel(entry.itemType);
  }

  function remoteItemTypeLabel(itemType: string) {
    const labels: Record<string, string> = {
      CollectionFolder: '媒体库',
      Series: '剧集',
      Season: '季',
      Movie: '电影',
      Episode: '单集',
      Video: '视频',
      Folder: '文件夹',
      BoxSet: '合集',
    };
    return labels[itemType] ?? itemType;
  }

  function usesLandscapeCover(entry: RemoteLibraryEntry) {
    return (
      entry.itemType === 'CollectionFolder' ||
      entry.itemType === 'Episode' ||
      (entry.imageAspectRatio ?? 0) >= 1.2
    );
  }

  function preferredSeason(detail: RemoteMediaDetail) {
    const resumableEpisode = detail.episodes.find(
      (episode) => episode.playbackPositionTicks > 0 && !episode.played,
    );
    const unfinishedEpisode =
      resumableEpisode ?? detail.episodes.find((episode) => !episode.played);
    return (
      detail.seasons.find(
        (season) =>
          season.id === unfinishedEpisode?.seasonId ||
          (season.indexNumber !== null &&
            season.indexNumber === unfinishedEpisode?.parentIndexNumber),
      ) ?? detail.seasons[0]
    );
  }

  function episodeBelongsToSeason(
    episode: RemoteEpisodeDetail,
    season: RemoteSeasonDetail,
  ) {
    return (
      episode.seasonId === season.id ||
      (season.indexNumber !== null &&
        episode.parentIndexNumber === season.indexNumber)
    );
  }

  function primaryPlayback(
    detail: RemoteMediaDetail,
    episodes: RemoteEpisodeDetail[],
  ) {
    if (detail.itemType !== 'Series') {
      return {
        id: detail.id,
        name: detail.name,
        positionTicks: detail.playbackPositionTicks,
        runtimeTicks: detail.runtimeTicks,
        played: detail.played,
        percentage: detail.playedPercentage,
      };
    }
    const episode =
      episodes.find(
        (entry) => entry.playbackPositionTicks > 0 && !entry.played,
      ) ??
      episodes.find((entry) => !entry.played) ??
      episodes[0] ??
      detail.episodes[0];
    return episode
      ? {
          id: episode.id,
          name: episode.name,
          positionTicks: episode.playbackPositionTicks,
          runtimeTicks: episode.runtimeTicks,
          played: episode.played,
          percentage: episode.playedPercentage,
        }
      : null;
  }

  function playbackButtonLabel(playable: ReturnType<typeof primaryPlayback>) {
    if (!playable) return '暂无可播放内容';
    if (playable.positionTicks > 0 && !playable.played) {
      return `继续播放 · ${formatTicks(playable.positionTicks)}`;
    }
    return playable.played ? '重新播放' : '播放';
  }

  function progressPercent(
    played: boolean,
    positionTicks: number,
    runtimeTicks: number | null,
    percentage: number | null,
  ) {
    if (played) return 100;
    if (percentage !== null) return Math.max(0, Math.min(100, percentage));
    if (!runtimeTicks || runtimeTicks <= 0) return 0;
    return Math.max(0, Math.min(100, (positionTicks / runtimeTicks) * 100));
  }

  function formatTicks(ticks: number | null) {
    if (!ticks || ticks <= 0) return '0:00';
    const seconds = Math.round(ticks / 10_000_000);
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    const remainder = seconds % 60;
    return hours > 0
      ? `${hours}:${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`
      : `${minutes}:${String(remainder).padStart(2, '0')}`;
  }

  function formatRuntime(ticks: number | null) {
    if (!ticks || ticks <= 0) return null;
    const minutes = Math.round(ticks / 10_000_000 / 60);
    return minutes >= 60
      ? `${Math.floor(minutes / 60)} 小时 ${minutes % 60} 分钟`
      : `${minutes} 分钟`;
  }

  function formatPremiereDate(value: string | null) {
    if (!value) return null;
    const date = new Date(value);
    return Number.isNaN(date.valueOf())
      ? null
      : new Intl.DateTimeFormat('zh-CN', {
          year: 'numeric',
          month: 'long',
          day: 'numeric',
        }).format(date);
  }

  function episodeNumber(episode: RemoteEpisodeDetail) {
    return episode.indexNumber === null
      ? '特别篇'
      : `E${String(episode.indexNumber).padStart(2, '0')}`;
  }

  function seasonProgress(season: RemoteSeasonDetail) {
    if (season.played) return '已看完';
    if (season.unplayedCount !== null) {
      return season.unplayedCount === 0
        ? '已看完'
        : `${season.unplayedCount} 集未看`;
    }
    return `${season.episodeCount} 集`;
  }

  function gridMaxWidth(count: number, cardWidth: number, gap: number) {
    return `${Math.max(0, count) * cardWidth + Math.max(0, count - 1) * gap}px`;
  }

  function emptyServerDraft(): MediaServerInput {
    return {
      kind: 'emby',
      name: '',
      baseUrl: '',
      username: '',
      password: '',
    };
  }

  function filterEntries<T>(
    entries: T[],
    value: string,
    searchable: (entry: T) => string,
  ) {
    const query = value.trim().toLocaleLowerCase();
    return query
      ? entries.filter((entry) => searchable(entry).includes(query))
      : entries;
  }

  function sortEntries<T extends { name: string }>(
    entries: T[],
    updatedAt: (entry: T) => number,
  ) {
    return [...entries].sort((left, right) => {
      if (librarySort.startsWith('name')) {
        const order = libraryNameCollator.compare(left.name, right.name);
        return librarySort === 'name-desc' ? -order : order;
      }

      const leftTime = updatedAt(left);
      const rightTime = updatedAt(right);
      if (leftTime <= 0 || rightTime <= 0) {
        if (leftTime <= 0 && rightTime > 0) return 1;
        if (rightTime <= 0 && leftTime > 0) return -1;
      }
      const timeOrder = leftTime - rightTime;
      if (timeOrder !== 0) {
        return librarySort === 'time-desc' ? -timeOrder : timeOrder;
      }
      return libraryNameCollator.compare(left.name, right.name);
    });
  }
</script>

<svelte:head>
  <title>mpv-enjoy Home</title>
</svelte:head>

<div class="app-shell" class:sidebar-collapsed={sidebarCollapsed}>
  <aside class="sidebar" id="primary-sidebar">
    <nav aria-label="主导航">
      <button
        class:active={view === 'home'}
        aria-label="首页"
        title="首页"
        onclick={() => (view = 'home')}
      >
        <Icon name="home" /><span>首页</span>
      </button>
      <button
        class:active={view === 'library'}
        aria-label={`媒体库，${sourceCount} 个媒体源`}
        title="媒体库"
        onclick={openLibrarySources}
      >
        <Icon name="library" /><span>媒体库</span>
        {#if sourceCount > 0}<em>{sourceCount}</em>{/if}
      </button>
      <button
        class:active={view === 'settings'}
        aria-label="设置"
        title="设置"
        onclick={() => (view = 'settings')}
      >
        <Icon name="settings" /><span>设置</span>
      </button>
    </nav>

    <button
      class="sidebar-toggle"
      type="button"
      aria-label={sidebarCollapsed ? '展开侧栏' : '收起侧栏'}
      aria-controls="primary-sidebar"
      aria-expanded={!sidebarCollapsed}
      title={sidebarCollapsed ? '展开侧栏' : '收起侧栏'}
      onclick={() => (sidebarCollapsed = !sidebarCollapsed)}
    >
      <Icon name="back" size={18} />
      <span>{sidebarCollapsed ? '展开侧栏' : '收起侧栏'}</span>
    </button>
    <button
      class="version-button"
      type="button"
      title={updateCheckResult?.hasUpdate
        ? `发现新版本 v${updateCheckResult.latestVersion}，点击前往设置更新`
        : '查看软件设置'}
      onclick={() => (view = 'settings')}
    >
      <span>
        mpv-enjoy Home{#if appVersion}
          · {appVersion}{/if}
      </span>
      {#if updateCheckResult?.hasUpdate}
        <span class="update-badge">新版本</span>
      {/if}
    </button>
  </aside>

  <main>
    {#if view === 'home'}
      <header class="page-header">
        <div>
          <span class="eyebrow">欢迎回来</span>
          <h1>今天想看点什么？</h1>
        </div>
        <div class="header-actions">
          <button class="secondary" onclick={showConnectServer}>
            <Icon name="server" />连接服务器
          </button>
          <button class="primary" onclick={addFolder}>
            <Icon name="add" />添加本地目录
          </button>
        </div>
      </header>

      <section class="content-section">
        <div class="section-heading">
          <div>
            <h2>媒体源</h2>
          </div>
        </div>

        {#if sourceCount === 0}
          <div class="empty-state">
            <div class="empty-icon"><Icon name="library" size={30} /></div>
            <h3>添加你的第一个媒体源</h3>
            <p>选择一个本地目录，或连接已有的 Emby / Jellyfin 服务器。</p>
            <div class="empty-actions">
              <button class="primary" onclick={addFolder}
                ><Icon name="add" />本地目录</button
              >
              <button class="secondary" onclick={showConnectServer}
                ><Icon name="server" />媒体服务器</button
              >
            </div>
          </div>
        {:else}
          <div
            class="source-grid"
            style:max-width={gridMaxWidth(sourceCount, 400, 14)}
          >
            {#each folders as folder (folder.id)}
              <article class="source-card">
                <button
                  class="source-main"
                  onclick={() => openLocalFolder(folder)}
                >
                  <span class="source-icon"
                    ><Icon name="folder" size={30} /></span
                  >
                  <span class="source-copy">
                    <span class="source-kind">本地目录</span>
                    <strong>{folder.name}</strong>
                    <small title={folder.path}>{folder.path}</small>
                    <span
                      >{folder.mediaCount} 个视频 · {formatScanTime(
                        folder.lastScannedAt,
                      )}</span
                    >
                  </span>
                  <span class="source-open"><Icon name="back" size={17} /></span
                  >
                </button>
                <div class="source-actions">
                  <button
                    title="重新扫描"
                    aria-label={`重新扫描 ${folder.name}`}
                    onclick={() => rescanFolder(folder)}
                    ><Icon name="refresh" size={17} /></button
                  >
                  <button
                    title="移除目录"
                    aria-label={`移除 ${folder.name}`}
                    onclick={() => removeFolder(folder)}
                    ><Icon name="trash" size={17} /></button
                  >
                </div>
              </article>
            {/each}
            {#each servers as server (server.id)}
              <article class="source-card server-card">
                <button
                  class="source-main"
                  onclick={() => openRemoteServer(server)}
                >
                  <span class="source-icon"
                    ><Icon name="server" size={28} /></span
                  >
                  <span class="source-copy">
                    <span class="source-kind"
                      >{serverKindLabel(server.kind)}</span
                    >
                    <strong>{server.name}</strong>
                    <small title={server.baseUrl}>{server.baseUrl}</small>
                    <span
                      >{server.userName}{server.serverVersion
                        ? ` · ${server.serverVersion}`
                        : ''}</span
                    >
                  </span>
                  <span class="source-open"><Icon name="back" size={17} /></span
                  >
                </button>
                <div class="source-actions">
                  <button
                    title="移除连接"
                    aria-label={`移除 ${server.name}`}
                    onclick={() => removeServer(server)}
                    ><Icon name="trash" size={17} /></button
                  >
                </div>
              </article>
            {/each}
          </div>
        {/if}
      </section>

      {#if recentMedia.length > 0}
        <section class="content-section">
          <div class="section-heading">
            <div>
              <h2>最近更新</h2>
            </div>
          </div>
          <div class="recent-grid">
            {#each recentMedia as item (item.key)}
              <button class="recent-card" onclick={() => openRecent(item)}>
                <span
                  class:remote={item.sourceKind === 'remote'}
                  class="recent-icon"
                >
                  <Icon
                    name={item.sourceKind === 'remote' ? 'server' : 'video'}
                    size={22}
                  />
                </span>
                <span class="recent-copy">
                  <strong>{item.name}</strong>
                  <small>{item.context}</small>
                  <span
                    >{item.sourceName} · {formatUpdateTime(
                      item.updatedAt,
                    )}</span
                  >
                </span>
                <Icon name="back" size={17} />
              </button>
            {/each}
          </div>
        </section>
      {/if}
    {:else if view === 'library'}
      <header class="page-header compact library-header">
        <div>
          {#if selectedSource}<span class="eyebrow">媒体库</span>{/if}
          <h1>
            {selectedSource?.kind === 'local'
              ? selectedSource.folder.name
              : selectedSource?.kind === 'remote'
                ? selectedSource.server.name
                : '媒体库'}
          </h1>
          {#if selectedSource}
            <p>
              {selectedSource.kind === 'local'
                ? selectedSource.folder.path
                : `${serverKindLabel(selectedSource.server.kind)} · ${selectedSource.server.userName}`}
            </p>
          {/if}
        </div>
        {#if selectedSource}
          <div class="header-actions horizontal">
            <button class="secondary" onclick={openLibrarySources}
              ><Icon name="back" />媒体源</button
            >
            {#if selectedSource.kind === 'local'}
              <button class="secondary" onclick={rescanSelectedFolder}
                ><Icon name="refresh" />重新扫描</button
              >
            {:else}
              <button
                class="secondary"
                onclick={() => navigateRemoteCrumb(remoteCrumbs.length - 1)}
                ><Icon name="refresh" />刷新</button
              >
            {/if}
          </div>
        {/if}
      </header>

      {#if sourceCount === 0}
        <div class="empty-state library-empty">
          <div class="empty-icon"><Icon name="library" size={30} /></div>
          <h3>媒体库还是空的</h3>
          <p>添加本地目录，或连接 Emby / Jellyfin 服务器。</p>
          <div class="empty-actions">
            <button class="primary" onclick={addFolder}
              ><Icon name="add" />添加目录</button
            >
            <button class="secondary" onclick={showConnectServer}
              ><Icon name="server" />连接服务器</button
            >
          </div>
        </div>
      {:else if !selectedSource}
        <section class="content-section source-picker-section">
          <div class="source-grid">
            {#each folders as folder (folder.id)}
              <article class="source-card">
                <button
                  class="source-main"
                  onclick={() => openLocalFolder(folder)}
                >
                  <span class="source-icon"
                    ><Icon name="folder" size={30} /></span
                  >
                  <span class="source-copy">
                    <span class="source-kind">本地目录</span>
                    <strong>{folder.name}</strong>
                    <small title={folder.path}>{folder.path}</small>
                    <span>{folder.mediaCount} 个视频</span>
                  </span>
                  <span class="source-open"><Icon name="back" size={17} /></span
                  >
                </button>
              </article>
            {/each}
            {#each servers as server (server.id)}
              <article class="source-card server-card">
                <button
                  class="source-main"
                  onclick={() => openRemoteServer(server)}
                >
                  <span class="source-icon"
                    ><Icon name="server" size={28} /></span
                  >
                  <span class="source-copy">
                    <span class="source-kind"
                      >{serverKindLabel(server.kind)}</span
                    >
                    <strong>{server.name}</strong>
                    <small title={server.baseUrl}>{server.baseUrl}</small>
                    <span>{server.userName}</span>
                  </span>
                  <span class="source-open"><Icon name="back" size={17} /></span
                  >
                </button>
              </article>
            {/each}
          </div>
        </section>
      {:else}
        <div class="library-toolbar context-toolbar">
          <div class="breadcrumbs" aria-label="当前位置">
            {#if selectedSource.kind === 'local'}
              {#each localBreadcrumbs() as crumb, index}
                {#if index > 0}<span>/</span>{/if}
                <button
                  class:current={index === localBreadcrumbs().length - 1}
                  onclick={() => navigateLocal(crumb.path)}>{crumb.name}</button
                >
              {/each}
            {:else}
              {#each remoteCrumbs as crumb, index}
                {#if index > 0}<span>/</span>{/if}
                <button
                  class:current={index === remoteCrumbs.length - 1}
                  onclick={() => navigateRemoteCrumb(index)}
                  >{crumb.name}</button
                >
              {/each}
            {/if}
          </div>
          {#if !remoteDetail}
            <div class="library-controls">
              <label class="sort-control">
                <span>排序</span>
                <select
                  aria-label="媒体库排序方式"
                  value={librarySort}
                  onchange={(event) =>
                    updateLibrarySort(event.currentTarget.value)}
                >
                  {#each librarySortOptions as option (option.value)}
                    <option value={option.value}>{option.label}</option>
                  {/each}
                </select>
              </label>
              <label class="search-box">
                <Icon name="search" size={18} />
                <input
                  bind:value={search}
                  type="search"
                  placeholder="搜索当前位置"
                  aria-label="搜索当前位置"
                />
                {#if search}<button
                    onclick={() => (search = '')}
                    aria-label="清除搜索">×</button
                  >{/if}
              </label>
            </div>
          {/if}
        </div>

        {#if libraryLoading}
          <div class="library-loading" role="status">
            <div class="spinner small"></div>
            <span>正在读取媒体库…</span>
          </div>
        {:else if selectedSource?.kind === 'local'}
          {#if localDirectories.length > 0}
            <section class="browser-section">
              <h2>文件夹</h2>
              <div
                class="browser-grid"
                style:max-width={gridMaxWidth(localDirectories.length, 300, 10)}
              >
                {#each localDirectories as entry (entry.key)}
                  <button
                    class="browser-card"
                    onclick={() => navigateLocal(entry.relativePath)}
                  >
                    <span class="browser-icon"
                      ><Icon name="folder" size={25} /></span
                    >
                    <span
                      ><strong>{entry.name}</strong><small
                        >{entry.mediaCount} 个视频</small
                      ></span
                    >
                    <Icon name="back" size={17} />
                  </button>
                {/each}
              </div>
            </section>
          {/if}
          {#if localVideos.length > 0}
            <section class="browser-section">
              <h2>{search ? '搜索结果' : '视频'}</h2>
              <div class="media-list clean-list">
                {#each localVideos as entry, index (entry.key)}
                  <button
                    class="media-list-item no-thumb"
                    onclick={() => playLocal(entry)}
                  >
                    <span class="list-index"
                      >{String(index + 1).padStart(2, '0')}</span
                    >
                    <span class="list-copy"
                      ><strong>{entry.name}</strong><small
                        >{entry.relativePath}</small
                      ></span
                    >
                    <em>{entry.extension?.toUpperCase()}</em>
                    <span class="list-play"><Icon name="play" size={14} /></span
                    >
                  </button>
                {/each}
              </div>
            </section>
          {/if}
          {#if visibleLocalEntries.length === 0}
            <div class="empty-state library-empty compact-empty">
              <div class="empty-icon"><Icon name="search" size={28} /></div>
              <h3>
                {search ? '当前位置没有匹配内容' : '这个文件夹里没有视频'}
              </h3>
              <p>
                {search
                  ? '换个关键词试试。'
                  : '返回上一级，或重新扫描媒体目录。'}
              </p>
            </div>
          {/if}
        {:else if selectedSource?.kind === 'remote'}
          {#if remoteDetail}
            {@const playable = primaryPlayback(remoteDetail, selectedEpisodes)}
            {@const detailProgress = playable
              ? progressPercent(
                  playable.played,
                  playable.positionTicks,
                  playable.runtimeTicks,
                  playable.percentage,
                )
              : 0}
            <article class="remote-detail">
              <section
                class:has-backdrop={!!remoteBackdrop}
                class="detail-hero"
              >
                {#if remoteBackdrop}
                  <img class="detail-backdrop" src={remoteBackdrop} alt="" />
                {/if}
                <div class="detail-hero-shade"></div>
                <div class="detail-hero-content">
                  <div class="detail-poster">
                    {#if remoteDetail.primaryImageId && remoteImages[remoteDetail.primaryImageId]}
                      <img
                        src={remoteImages[remoteDetail.primaryImageId]}
                        alt={`${remoteDetail.name} 封面`}
                      />
                    {:else}
                      <Icon name="video" size={34} />
                    {/if}
                  </div>
                  <div class="detail-copy">
                    <div class="detail-labels">
                      <span>{remoteItemTypeLabel(remoteDetail.itemType)}</span>
                      {#if remoteDetail.played}<span class="watched"
                          >✓ 已看完</span
                        >{:else if detailProgress > 0}<span
                          >已播放 {Math.round(detailProgress)}%</span
                        >{/if}
                    </div>
                    <h2>{remoteDetail.name}</h2>
                    {#if remoteDetail.tagline}
                      <p class="tagline">{remoteDetail.tagline}</p>
                    {/if}
                    <div class="detail-meta">
                      {#if remoteDetail.productionYear}<span
                          >{remoteDetail.productionYear}</span
                        >{/if}
                      {#if formatRuntime(remoteDetail.runtimeTicks)}<span
                          >{formatRuntime(remoteDetail.runtimeTicks)}</span
                        >{/if}
                      {#if remoteDetail.officialRating}<span
                          >{remoteDetail.officialRating}</span
                        >{/if}
                      {#if remoteDetail.communityRating !== null}<span
                          >★ {remoteDetail.communityRating.toFixed(1)}</span
                        >{/if}
                      {#each remoteDetail.genres.slice(0, 4) as genre}<span
                          >{genre}</span
                        >{/each}
                    </div>
                    {#if remoteDetail.overview}
                      <p class="detail-overview">{remoteDetail.overview}</p>
                    {/if}
                    <div class="detail-playback">
                      <button
                        class="primary detail-play"
                        disabled={!playable}
                        onclick={() =>
                          playable &&
                          playRemoteItem(playable.id, playable.name)}
                      >
                        <Icon name="play" size={16} />{playbackButtonLabel(
                          playable,
                        )}
                      </button>
                      {#if playable && detailProgress > 0}
                        <div
                          class="watch-progress"
                          aria-label={`播放进度 ${Math.round(detailProgress)}%`}
                        >
                          <span style:width={`${detailProgress}%`}></span>
                        </div>
                      {/if}
                    </div>
                  </div>
                </div>
              </section>

              {#if remoteDetail.itemType === 'Series' && remoteDetail.seasons.length > 0}
                <section class="detail-section season-section">
                  <div class="detail-section-heading">
                    <div>
                      <span class="eyebrow">季度与单集</span>
                      <h3>单集</h3>
                    </div>
                    {#if remoteDetail.seasons.length > 1}
                      <label class="season-selector">
                        <span>季度</span>
                        <select
                          aria-label="选择季度"
                          value={selectedSeason?.id ?? ''}
                          onchange={(event) =>
                            void selectRemoteSeason(event.currentTarget.value)}
                        >
                          {#each remoteDetail.seasons as season (season.id)}
                            <option value={season.id}>{season.name}</option>
                          {/each}
                        </select>
                      </label>
                    {/if}
                  </div>

                  {#if selectedSeason}
                    <div class="season-summary">
                      <div class="season-poster">
                        {#if selectedSeason.primaryImageId && remoteImages[selectedSeason.primaryImageId]}
                          <img
                            src={remoteImages[selectedSeason.primaryImageId]}
                            alt={`${selectedSeason.name} 封面`}
                          />
                        {:else if remoteDetail.primaryImageId && remoteImages[remoteDetail.primaryImageId]}
                          <img
                            src={remoteImages[remoteDetail.primaryImageId]}
                            alt={`${selectedSeason.name} 封面`}
                          />
                        {:else}
                          <Icon name="library" size={28} />
                        {/if}
                      </div>
                      <div>
                        <span class="season-state"
                          >{seasonProgress(selectedSeason)}</span
                        >
                        <h4>{selectedSeason.name}</h4>
                        <p>
                          {selectedSeason.overview ??
                            '服务器尚未提供这一季的简介。系列简介仍保留在页面上方。'}
                        </p>
                      </div>
                    </div>
                  {/if}

                  <div class="episode-grid">
                    {#each selectedEpisodes as episode (episode.id)}
                      {@const episodeProgress = progressPercent(
                        episode.played,
                        episode.playbackPositionTicks,
                        episode.runtimeTicks,
                        episode.playedPercentage,
                      )}
                      <button
                        class:played={episode.played}
                        class="episode-card"
                        onclick={() => playRemoteItem(episode.id, episode.name)}
                      >
                        <span class="episode-art">
                          {#if episode.primaryImageId && remoteImages[episode.primaryImageId]}
                            <img
                              src={remoteImages[episode.primaryImageId]}
                              alt=""
                            />
                          {:else}
                            <Icon name="play" size={22} />
                          {/if}
                          {#if episode.played}<i>✓</i>{/if}
                          {#if episodeProgress > 0}
                            <span class="episode-progress"
                              ><b style:width={`${episodeProgress}%`}></b></span
                            >
                          {/if}
                        </span>
                        <span class="episode-copy">
                          <span class="episode-meta"
                            >{episodeNumber(episode)}{formatRuntime(
                              episode.runtimeTicks,
                            )
                              ? ` · ${formatRuntime(episode.runtimeTicks)}`
                              : ''}</span
                          >
                          <strong>{episode.name}</strong>
                          {#if episode.overview}<small>{episode.overview}</small
                            >{:else if formatPremiereDate(episode.premiereDate)}<small
                              >{formatPremiereDate(episode.premiereDate)}</small
                            >{/if}
                        </span>
                      </button>
                    {/each}
                  </div>
                </section>
              {/if}

              {#if remoteDetail.people.length > 0}
                <section class="detail-section people-section">
                  <div class="detail-section-heading">
                    <div>
                      <span class="eyebrow">演职人员</span>
                      <h3>主创与演员</h3>
                    </div>
                  </div>
                  <div class="people-row">
                    {#each remoteDetail.people.slice(0, 30) as person, index (`${person.id ?? person.name}-${index}`)}
                      <div class="person-card">
                        <span>
                          {#if person.primaryImageId && remoteImages[person.primaryImageId]}
                            <img
                              src={remoteImages[person.primaryImageId]}
                              alt=""
                            />
                          {:else}{person.name.slice(0, 1)}{/if}
                        </span>
                        <strong>{person.name}</strong>
                        <small
                          >{person.role ??
                            person.personType ??
                            '演职人员'}</small
                        >
                      </div>
                    {/each}
                  </div>
                </section>
              {/if}
            </article>
          {:else if visibleRemoteEntries.length > 0}
            <div
              class="remote-grid"
              style:max-width={gridMaxWidth(
                visibleRemoteEntries.length,
                260,
                12,
              )}
            >
              {#each visibleRemoteEntries as entry (entry.id)}
                <button
                  class:has-cover={!!remoteImages[entry.id]}
                  class:landscape={usesLandscapeCover(entry)}
                  class="remote-card"
                  onclick={() =>
                    entry.kind === 'collection'
                      ? openRemoteCollection(entry)
                      : openRemoteDetail(entry)}
                >
                  {#if remoteImages[entry.id]}
                    <img src={remoteImages[entry.id]} alt="" />
                  {/if}
                  {#if entry.itemType === 'Series' && (entry.unplayedCount ?? 0) > 0}
                    <span
                      class="remote-unplayed-badge"
                      aria-label={`${entry.unplayedCount} 集未看`}
                      title={`${entry.unplayedCount} 集未看`}
                      >{entry.unplayedCount}</span
                    >
                  {/if}
                  <span class="remote-copy">
                    <span class="source-kind">{remoteTypeLabel(entry)}</span>
                    <strong>{entry.name}</strong>
                    {#if entry.subtitle}<small>{entry.subtitle}</small>{/if}
                    {#if entry.kind === 'collection' && entry.childCount > 0}<span
                        >{entry.childCount} 项</span
                      >{/if}
                  </span>
                  <span
                    class:collection={entry.kind === 'collection'}
                    class="remote-action"><Icon name="back" size={16} /></span
                  >
                </button>
              {/each}
            </div>
          {:else}
            <div class="empty-state library-empty compact-empty">
              <div class="empty-icon"><Icon name="search" size={28} /></div>
              <h3>
                {search ? '当前位置没有匹配内容' : '服务器没有返回视频内容'}
              </h3>
              <p>
                {search
                  ? '换个关键词试试。'
                  : '检查这个用户是否有对应媒体库的访问权限。'}
              </p>
            </div>
          {/if}
        {/if}
      {/if}
    {:else}
      <header class="page-header compact">
        <div>
          <h1>设置</h1>
        </div>
      </header>

      <section class="settings-card appearance-card">
        <div class="settings-icon appearance-icon">
          <Icon name="sparkles" size={22} />
        </div>
        <div class="settings-copy">
          <div class="settings-title">
            <h2>外观</h2>
          </div>
          <div class="appearance-settings">
            <div class="appearance-row">
              <span class="appearance-label">主题模式</span>
              <div class="theme-options" aria-label="主题模式">
                {#each themeOptions as option (option.value)}
                  <button
                    type="button"
                    class:active={appearance.themeMode === option.value}
                    aria-pressed={appearance.themeMode === option.value}
                    disabled={appearanceSaving}
                    onclick={() =>
                      void updateAppearance({
                        ...appearance,
                        themeMode: option.value,
                      })}
                  >
                    {option.label}
                  </button>
                {/each}
              </div>
            </div>
            <div class="appearance-row">
              <span class="appearance-label">主题色</span>
              <div class="accent-options" aria-label="主题色">
                {#each accentOptions as option (option.value)}
                  <button
                    type="button"
                    class:active={appearance.accentColor === option.value}
                    data-color={option.value}
                    aria-label={`${option.label}主题色`}
                    aria-pressed={appearance.accentColor === option.value}
                    title={option.label}
                    disabled={appearanceSaving}
                    onclick={() =>
                      void updateAppearance({
                        ...appearance,
                        accentColor: option.value,
                      })}
                  >
                    <span></span><em>{option.label}</em>
                  </button>
                {/each}
              </div>
            </div>
          </div>
        </div>
      </section>

      <section class="settings-card">
        <div class="settings-icon"><Icon name="play" size={22} /></div>
        <div class="settings-copy">
          <div class="settings-title">
            <h2>播放器</h2>
            <span class:available={player?.available}
              >{player?.available ? '可用' : '未找到'}</span
            >
          </div>
          <dl>
            <div>
              <dt>来源</dt>
              <dd>{player ? sourceLabel(player.source) : '正在检测'}</dd>
            </div>
            <div>
              <dt>路径</dt>
              <dd title={player?.executable ?? ''}>
                {player?.executable ?? '尚未选择，且系统中没有发现 mpv'}
              </dd>
            </div>
          </dl>
          <div class="player-preferences">
            <div class="preference-row">
              <span class="preference-copy">
                <strong>启动音量</strong>
              </span>
              <div class="volume-preference">
                <label class="override-toggle">
                  <input
                    type="checkbox"
                    checked={playerPreferences.startupVolume !== null}
                    disabled={playerPreferencesSaving}
                    onchange={(event) =>
                      void updatePlayerPreferences({
                        ...playerPreferences,
                        startupVolume: event.currentTarget.checked
                          ? volumeDraft
                          : null,
                      })}
                  />
                  <span>自定义</span>
                </label>
                <input
                  type="range"
                  min="0"
                  max="100"
                  step="1"
                  value={volumeDraft}
                  aria-label="播放器启动音量"
                  disabled={playerPreferences.startupVolume === null ||
                    playerPreferencesSaving}
                  oninput={(event) =>
                    (volumeDraft = Number(event.currentTarget.value))}
                  onchange={() =>
                    void updatePlayerPreferences({
                      ...playerPreferences,
                      startupVolume: volumeDraft,
                    })}
                />
                <output
                  >{playerPreferences.startupVolume === null
                    ? '跟随 mpv'
                    : `${volumeDraft}%`}</output
                >
              </div>
            </div>
            <label class="preference-row">
              <span class="preference-copy">
                <strong>全屏启动</strong>
              </span>
              <select
                class="preference-select"
                value={playerPreferences.fullscreenMode}
                disabled={playerPreferencesSaving}
                onchange={(event) =>
                  void updatePlayerPreferences({
                    ...playerPreferences,
                    fullscreenMode: event.currentTarget
                      .value as PlayerToggleMode,
                  })}
              >
                {#each playerToggleOptions as option (option.value)}
                  <option value={option.value}>{option.label}</option>
                {/each}
              </select>
            </label>
            <label class="preference-row">
              <span class="preference-copy">
                <strong>uosc_danmaku 弹幕</strong>
                <small>插件不可用时会忽略此项，不影响播放</small>
              </span>
              <select
                class="preference-select"
                value={playerPreferences.danmakuMode}
                disabled={playerPreferencesSaving}
                onchange={(event) =>
                  void updatePlayerPreferences({
                    ...playerPreferences,
                    danmakuMode: event.currentTarget.value as PlayerToggleMode,
                  })}
              >
                {#each playerToggleOptions as option (option.value)}
                  <option value={option.value}>{option.label}</option>
                {/each}
              </select>
            </label>
            <details class="danmaku-style-preferences">
              <summary>
                <span class="preference-copy">
                  <strong>弹幕样式</strong>
                </span>
                <span class="details-hint">详情</span>
              </summary>
              <div class="danmaku-style-rows">
                <div class="danmaku-style-row">
                  <span class="preference-copy">
                    <strong>粗体</strong>
                    <small>控制弹幕文字是否加粗</small>
                  </span>
                  <select
                    class="preference-select"
                    value={playerPreferences.danmakuStyle.boldMode}
                    aria-label="弹幕粗体"
                    disabled={playerPreferencesSaving}
                    onchange={(event) =>
                      void updateDanmakuStyle({
                        boldMode: event.currentTarget.value as PlayerToggleMode,
                      })}
                  >
                    {#each danmakuToggleOptions as option (option.value)}
                      <option value={option.value}>{option.label}</option>
                    {/each}
                  </select>
                </div>
                <PreferenceRange
                  label="字号"
                  description="控制弹幕字体大小"
                  overridden={playerPreferences.danmakuStyle.fontSize !== null}
                  draft={danmakuFontSizeDraft}
                  min={10}
                  max={100}
                  step={1}
                  output={String(danmakuFontSizeDraft)}
                  disabled={playerPreferencesSaving}
                  onToggle={(enabled) =>
                    void updateDanmakuStyle({
                      fontSize: enabled ? danmakuFontSizeDraft : null,
                    })}
                  onDraft={(value) => (danmakuFontSizeDraft = value)}
                  onCommit={() =>
                    void updateDanmakuStyle({
                      fontSize: danmakuFontSizeDraft,
                    })}
                />
                <PreferenceRange
                  label="描边"
                  description="描边粗细，范围 0–4"
                  overridden={playerPreferences.danmakuStyle.outline !== null}
                  draft={danmakuOutlineDraft}
                  min={0}
                  max={4}
                  step={0.1}
                  output={danmakuOutlineDraft.toFixed(1)}
                  disabled={playerPreferencesSaving}
                  onToggle={(enabled) =>
                    void updateDanmakuStyle({
                      outline: enabled ? danmakuOutlineDraft : null,
                    })}
                  onDraft={(value) => (danmakuOutlineDraft = value)}
                  onCommit={() =>
                    void updateDanmakuStyle({
                      outline: danmakuOutlineDraft,
                    })}
                />
                <PreferenceRange
                  label="阴影"
                  description="阴影深度，范围 0–10"
                  overridden={playerPreferences.danmakuStyle.shadow !== null}
                  draft={danmakuShadowDraft}
                  min={0}
                  max={10}
                  step={1}
                  output={String(danmakuShadowDraft)}
                  disabled={playerPreferencesSaving}
                  onToggle={(enabled) =>
                    void updateDanmakuStyle({
                      shadow: enabled ? danmakuShadowDraft : null,
                    })}
                  onDraft={(value) => (danmakuShadowDraft = value)}
                  onCommit={() =>
                    void updateDanmakuStyle({
                      shadow: danmakuShadowDraft,
                    })}
                />
                <PreferenceRange
                  label="滚动时长"
                  description="数值越大，滚动弹幕移动越慢"
                  overridden={playerPreferences.danmakuStyle.scrollTime !==
                    null}
                  draft={danmakuScrollTimeDraft}
                  min={1}
                  max={60}
                  step={1}
                  output={`${danmakuScrollTimeDraft} 秒`}
                  disabled={playerPreferencesSaving}
                  onToggle={(enabled) =>
                    void updateDanmakuStyle({
                      scrollTime: enabled ? danmakuScrollTimeDraft : null,
                    })}
                  onDraft={(value) => (danmakuScrollTimeDraft = value)}
                  onCommit={() =>
                    void updateDanmakuStyle({
                      scrollTime: danmakuScrollTimeDraft,
                    })}
                />
                <PreferenceRange
                  label="不透明度"
                  description="0% 完全透明，100% 完全不透明"
                  overridden={playerPreferences.danmakuStyle.opacity !== null}
                  draft={danmakuOpacityDraft}
                  min={0}
                  max={1}
                  step={0.05}
                  output={`${Math.round(danmakuOpacityDraft * 100)}%`}
                  disabled={playerPreferencesSaving}
                  onToggle={(enabled) =>
                    void updateDanmakuStyle({
                      opacity: enabled ? danmakuOpacityDraft : null,
                    })}
                  onDraft={(value) => (danmakuOpacityDraft = value)}
                  onCommit={() =>
                    void updateDanmakuStyle({
                      opacity: danmakuOpacityDraft,
                    })}
                />
                <PreferenceRange
                  label="显示区域"
                  description="限制弹幕占用的画面高度"
                  overridden={playerPreferences.danmakuStyle.displayArea !==
                    null}
                  draft={danmakuDisplayAreaDraft}
                  min={0}
                  max={1}
                  step={0.05}
                  output={`${Math.round(danmakuDisplayAreaDraft * 100)}%`}
                  disabled={playerPreferencesSaving}
                  onToggle={(enabled) =>
                    void updateDanmakuStyle({
                      displayArea: enabled ? danmakuDisplayAreaDraft : null,
                    })}
                  onDraft={(value) => (danmakuDisplayAreaDraft = value)}
                  onCommit={() =>
                    void updateDanmakuStyle({
                      displayArea: danmakuDisplayAreaDraft,
                    })}
                />
              </div>
            </details>
          </div>
          <div class="settings-actions">
            <button class="primary" onclick={choosePlayer}>选择播放器</button
            ><button class="secondary" onclick={resetPlayer}
              >恢复自动发现</button
            >
          </div>
        </div>
      </section>

      <section class="settings-card update-card">
        <div class="settings-icon update-icon">
          <Icon name="download" size={22} />
        </div>
        <div class="settings-copy">
          <div class="settings-title">
            <h2>软件更新</h2>
            {#if updateCheckResult}
              <span class:available={updateCheckResult.hasUpdate}>
                {updateCheckResult.hasUpdate
                  ? `发现新版本 v${updateCheckResult.latestVersion}`
                  : '已是最新版本'}
              </span>
            {/if}
          </div>

          <div class="update-summary-row">
            <span class="update-app-info">
              <strong
                >{updateCheckResult?.distributionName ??
                  'mpv-enjoy Home'}</strong
              >
              {#if appVersion}
                <span class="current-version-tag">当前版本: v{appVersion}</span>
              {/if}
              {#if updateCheckResult}
                <span class="install-type-tag"
                  >{installTypeLabel(updateCheckResult.installType)}</span
                >
              {/if}
            </span>
          </div>

          {#if updateCheckResult?.hasUpdate}
            <div class="update-detail-panel">
              <div class="update-detail-header">
                <div>
                  <span class="new-version-badge"
                    >最新版本：v{updateCheckResult.latestVersion}</span
                  >
                  {#if updateCheckResult.publishedAt}
                    <small class="update-date"
                      >{new Date(
                        updateCheckResult.publishedAt,
                      ).toLocaleDateString('zh-CN')}</small
                    >
                  {/if}
                </div>
              </div>

              {#if updateCheckResult.releaseNotes}
                <div class="update-notes">
                  <pre>{updateCheckResult.releaseNotes}</pre>
                </div>
              {/if}

              {#if updateCheckResult.matchedAsset}
                <div class="matched-asset-info">
                  <span class="asset-name"
                    >{updateCheckResult.matchedAsset.name}</span
                  >
                  <span class="asset-size"
                    >({formatBytes(updateCheckResult.matchedAsset.size)})</span
                  >
                </div>
              {/if}
            </div>
          {/if}

          <div class="settings-actions">
            {#if updateCheckResult?.hasUpdate && updateCheckResult.matchedAsset}
              <button class="primary" disabled={updating} onclick={applyUpdate}>
                <Icon name="download" />
                {#if updateCheckResult.installType === 'mac-app'}
                  {updating ? '正在下载...' : '下载并打开 DMG'}
                {:else if updateCheckResult.installType === 'windows-setup'}
                  {updating ? '正在下载...' : '下载并运行安装'}
                {:else}
                  {updating ? '正在更新...' : '立即覆盖更新并重启'}
                {/if}
              </button>
            {/if}

            <button
              class="secondary"
              disabled={checkingUpdate || updating}
              onclick={() => void checkUpdate(false)}
            >
              <Icon name="refresh" />
              {checkingUpdate ? '正在检查...' : '检查更新'}
            </button>

            {#if updateCheckResult?.releaseUrl}
              <button
                class="secondary"
                type="button"
                onclick={() => openExternal(updateCheckResult!.releaseUrl)}
              >
                前往 Release 页面
              </button>
            {/if}
          </div>
        </div>
      </section>

      <section class="settings-card">
        <div class="settings-icon server-icon">
          <Icon name="server" size={22} />
        </div>
        <div class="settings-copy">
          <div class="settings-title">
            <h2>Emby / Jellyfin</h2>
            <span class:available={servers.length > 0}
              >{servers.length > 0
                ? `${servers.length} 个连接`
                : '未连接'}</span
            >
          </div>

          {#if servers.length > 0}
            <div class="server-list">
              {#each servers as server (server.id)}
                <div>
                  <span class="server-badge"
                    >{serverKindLabel(server.kind)}</span
                  >
                  <span class="server-list-copy"
                    ><strong>{server.name}</strong><small
                      >{server.baseUrl} · {server.userName}</small
                    ></span
                  >
                  <button
                    aria-label={`打开 ${server.name}`}
                    title="打开媒体库"
                    onclick={() => openRemoteServer(server)}
                    ><Icon name="library" size={17} /></button
                  >
                  <button
                    aria-label={`重新登录 ${server.name}`}
                    title="重新登录"
                    onclick={() => showServerReauthentication(server)}
                    ><Icon name="refresh" size={17} /></button
                  >
                  <button
                    aria-label={`移除 ${server.name}`}
                    title="移除连接"
                    onclick={() => removeServer(server)}
                    ><Icon name="trash" size={17} /></button
                  >
                </div>
              {/each}
            </div>
          {/if}

          {#if showServerForm}
            <form
              class="server-form"
              onsubmit={(event) => {
                event.preventDefault();
                void connectServer();
              }}
            >
              <p class="form-note wide">
                {#if reauthenticatingServer}
                  正在更新“{reauthenticatingServer.name}”的登录凭据，连接记录和媒体库入口会保留。
                {/if}
                密码会保存在本机应用数据中，不使用系统钥匙串，仅供后端在令牌失效时自动重新登录。
              </p>
              <label
                ><span>服务类型</span><select
                  bind:value={serverDraft.kind}
                  disabled={!!reauthenticatingServer}
                  ><option value="emby">Emby</option><option value="jellyfin"
                    >Jellyfin</option
                  ></select
                ></label
              >
              <label
                ><span>显示名称（可选）</span><input
                  bind:value={serverDraft.name}
                  readonly={!!reauthenticatingServer}
                  placeholder="例如：客厅媒体库"
                /></label
              >
              <label class="wide"
                ><span>服务器地址</span><input
                  bind:value={serverDraft.baseUrl}
                  type="url"
                  required
                  readonly={!!reauthenticatingServer}
                  placeholder="http://192.168.1.20:8096"
                /></label
              >
              <label
                ><span>用户名</span><input
                  bind:value={serverDraft.username}
                  required
                  autocomplete="username"
                  placeholder="Emby / Jellyfin 用户名"
                /></label
              >
              <label
                ><span>密码</span><input
                  bind:value={serverDraft.password}
                  type="password"
                  autocomplete="current-password"
                  placeholder="无密码账户可留空"
                /></label
              >
              <div class="form-actions wide">
                <button
                  type="button"
                  class="secondary"
                  onclick={closeServerForm}>取消</button
                ><button type="submit" class="primary"
                  >{reauthenticatingServer ? '重新登录' : '测试并保存'}</button
                >
              </div>
            </form>
          {:else}
            <div class="settings-actions">
              <button class="primary" onclick={showConnectServer}
                ><Icon name="add" />连接媒体服务器</button
              >
            </div>
          {/if}
        </div>
      </section>

      <section class="settings-card subtle-card">
        <div class="settings-icon soft"><Icon name="library" size={22} /></div>
        <div class="settings-copy">
          <div class="settings-title">
            <h2>本地媒体索引</h2>
          </div>
          <dl>
            <div>
              <dt>目录</dt>
              <dd>{folders.length}</dd>
            </div>
            <div>
              <dt>媒体条目</dt>
              <dd>{totalMedia}</dd>
            </div>
          </dl>
        </div>
      </section>
    {/if}
  </main>
</div>

{#if busy}
  <div class="busy-layer" role="status" aria-live="polite">
    <div class="spinner"></div>
    <strong>{busyMessage}</strong><span>网络连接或大目录可能需要一点时间</span>
  </div>
{/if}

{#if toast}
  <div
    class:error={toast.kind === 'error'}
    class="toast"
    role="status"
    aria-live="polite"
  >
    <span>{toast.kind === 'success' ? '✓' : '!'}</span>{toast.message}
  </div>
{/if}
