<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { onMount } from 'svelte';
  import { api } from './api';
  import Icon from './components/Icon.svelte';
  import type {
    FolderSummary,
    LibraryEntry,
    MediaServerInput,
    MediaServerSummary,
    PlayerStatus,
    RecentCollection,
    RemoteEpisodeDetail,
    RemoteLibraryEntry,
    RemoteMediaDetail,
    RemoteSeasonDetail,
  } from './types';

  type View = 'home' | 'library' | 'settings';
  type Toast = { kind: 'success' | 'error'; message: string };
  type LibrarySource =
    | { kind: 'local'; folder: FolderSummary }
    | { kind: 'remote'; server: MediaServerSummary };
  type RemoteCrumb = {
    id: string | null;
    name: string;
    mode: 'list' | 'detail';
  };

  let view = $state<View>('home');
  let folders = $state<FolderSummary[]>([]);
  let servers = $state<MediaServerSummary[]>([]);
  let recentCollections = $state<RecentCollection[]>([]);
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
  let search = $state('');
  let busy = $state(false);
  let libraryLoading = $state(false);
  let busyMessage = $state('正在整理媒体库…');
  let toast = $state<Toast | null>(null);
  let showServerForm = $state(false);
  let serverDraft = $state<MediaServerInput>(emptyServerDraft());
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let remoteLoadGeneration = 0;

  const totalMedia = $derived(
    folders.reduce((total, folder) => total + folder.mediaCount, 0),
  );
  const sourceCount = $derived(folders.length + servers.length);
  const visibleLocalEntries = $derived(
    filterEntries(localEntries, search, (entry) =>
      `${entry.name} ${entry.relativePath}`.toLocaleLowerCase(),
    ),
  );
  const localDirectories = $derived(
    visibleLocalEntries.filter((entry) => entry.kind === 'folder'),
  );
  const localVideos = $derived(
    visibleLocalEntries.filter((entry) => entry.kind === 'video'),
  );
  const visibleRemoteEntries = $derived(
    filterEntries(remoteEntries, search, (entry) =>
      `${entry.name} ${entry.subtitle ?? ''}`.toLocaleLowerCase(),
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

  onMount(async () => {
    await refreshOverview();
  });

  async function refreshOverview() {
    try {
      [folders, servers, recentCollections, player] = await Promise.all([
        api.listFolders(),
        api.listMediaServers(),
        api.listRecentCollections(8),
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
    showServerForm = true;
  }

  async function connectServer() {
    await withBusy(
      `正在连接 ${serverKindLabel(serverDraft.kind)}…`,
      async () => {
        const usesPassword = serverDraft.authMode === 'password';
        const server = await api.addMediaServer({
          kind: serverDraft.kind,
          authMode: serverDraft.authMode,
          name: serverDraft.name.trim(),
          baseUrl: serverDraft.baseUrl.trim(),
          token: usesPassword ? '' : serverDraft.token.trim(),
          userId: usesPassword
            ? undefined
            : serverDraft.userId?.trim() || undefined,
          username: usesPassword ? serverDraft.username.trim() : '',
          password: usesPassword ? serverDraft.password : '',
        });
        serverDraft = emptyServerDraft();
        showServerForm = false;
        await refreshOverview();
        notify('success', `已连接“${server.name}”`);
      },
    );
  }

  async function openLocalFolder(folder: FolderSummary, path = '') {
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
    try {
      localEntries = await api.listLibraryEntries(folder.id, path);
    } catch (error) {
      notify('error', normalizeError(error));
    } finally {
      libraryLoading = false;
    }
  }

  async function openRemoteServer(server: MediaServerSummary) {
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
      notify('error', normalizeError(error));
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
      notify('error', normalizeError(error));
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
    const targets = entries.filter((entry) => entry.hasImage).slice(0, 40);
    await loadRemoteImageSet(
      serverId,
      targets.map((entry) => entry.id),
      generation,
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
    } catch {
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
        } catch {
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
    try {
      await api.playRemoteMedia(selectedSource.server.id, itemId);
      notify('success', `已交给 mpv 播放：${name}`);
    } catch (error) {
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

  function openFirstLibrary() {
    if (selectedSource?.kind === 'local') {
      void openLocalFolder(selectedSource.folder, currentPath);
    } else if (selectedSource?.kind === 'remote') {
      void openRemoteServer(selectedSource.server);
    } else if (folders[0]) {
      void openLocalFolder(folders[0]);
    } else if (servers[0]) {
      void openRemoteServer(servers[0]);
    } else {
      view = 'library';
    }
  }

  function openRecent(collection: RecentCollection) {
    const folder = folders.find((entry) => entry.id === collection.folderId);
    if (folder) void openLocalFolder(folder, collection.relativePath);
  }

  function navigateLocal(path: string) {
    if (selectedSource?.kind === 'local') {
      void openLocalFolder(selectedSource.folder, path);
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

  function normalizeError(error: unknown) {
    return typeof error === 'string'
      ? error
      : error instanceof Error
        ? error.message
        : '发生未知错误';
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
      kind: 'jellyfin',
      authMode: 'password',
      name: '',
      baseUrl: '',
      token: '',
      userId: '',
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
</script>

<svelte:head>
  <title>mpv-enjoy Home</title>
</svelte:head>

<div class="app-shell">
  <aside class="sidebar">
    <div class="brand">
      <div class="brand-mark"><Icon name="play" size={18} /></div>
      <div><strong>mpv-enjoy</strong><span>Home</span></div>
    </div>

    <nav aria-label="主导航">
      <button class:active={view === 'home'} onclick={() => (view = 'home')}>
        <Icon name="home" /><span>首页</span>
      </button>
      <button class:active={view === 'library'} onclick={openFirstLibrary}>
        <Icon name="library" /><span>媒体库</span>
        {#if sourceCount > 0}<em>{sourceCount}</em>{/if}
      </button>
      <button
        class:active={view === 'settings'}
        onclick={() => (view = 'settings')}
      >
        <Icon name="settings" /><span>设置</span>
        <i
          class:ready={player?.available}
          aria-label={player?.available ? '播放器可用' : '播放器未配置'}
        ></i>
      </button>
    </nav>

    <div class="sidebar-note">
      <Icon name="sparkles" size={18} />
      <p>统一整理本地目录、Emby 与 Jellyfin，播放仍完整交给你的 mpv。</p>
    </div>
    <span class="version">Technical preview · 0.1.0</span>
  </aside>

  <main>
    {#if view === 'home'}
      <header class="page-header">
        <div>
          <span class="eyebrow">欢迎回来</span>
          <h1>今天想看点什么？</h1>
          <p>本地目录与家庭媒体服务器，都在同一个入口。</p>
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

      <section class="hero">
        <div class="hero-copy">
          <span class="hero-label"
            ><Icon name="sparkles" size={16} /> 一个媒体首页</span
          >
          <h2>按作品浏览，不再被文件淹没。</h2>
          <p>保留本地文件夹层级，也理解 Emby / Jellyfin 的剧集与季度结构。</p>
          <div class="hero-stats">
            <div><strong>{sourceCount}</strong><span>个媒体源</span></div>
            <div><strong>{totalMedia}</strong><span>个本地视频</span></div>
            <div>
              <strong class:status-good={player?.available}>
                {player?.available ? '就绪' : '待设置'}
              </strong>
              <span>播放器</span>
            </div>
          </div>
        </div>
        <div class="hero-art" aria-hidden="true">
          <div class="orb orb-one"></div>
          <div class="orb orb-two"></div>
          <div class="play-tile"><Icon name="play" size={42} /></div>
        </div>
      </section>

      <section class="content-section">
        <div class="section-heading">
          <div>
            <h2>媒体源</h2>
            <p>本地目录与家庭媒体服务器</p>
          </div>
          {#if sourceCount > 0}
            <div class="inline-actions">
              <button class="text-button" onclick={showConnectServer}
                >连接服务器</button
              >
              <button class="text-button" onclick={addFolder}
                >添加目录 <Icon name="add" size={16} /></button
              >
            </div>
          {/if}
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

      {#if recentCollections.length > 0}
        <section class="content-section">
          <div class="section-heading">
            <div>
              <h2>最近更新的收藏</h2>
              <p>同一目录或剧集只显示一次</p>
            </div>
          </div>
          <div class="recent-grid">
            {#each recentCollections as collection (collection.key)}
              <button
                class="recent-card"
                onclick={() => openRecent(collection)}
              >
                <span class="recent-icon"><Icon name="folder" size={22} /></span
                >
                <span class="recent-copy">
                  <strong>{collection.name}</strong>
                  <small>{collection.latestMediaName}</small>
                  <span>{collection.mediaCount} 个视频</span>
                </span>
                <Icon name="back" size={17} />
              </button>
            {/each}
          </div>
        </section>
      {/if}
    {:else if view === 'library'}
      <header class="page-header compact">
        <div>
          <span class="eyebrow">媒体库</span>
          <h1>
            {selectedSource?.kind === 'local'
              ? selectedSource.folder.name
              : selectedSource?.kind === 'remote'
                ? selectedSource.server.name
                : '选择一个媒体源'}
          </h1>
          <p>
            {selectedSource?.kind === 'local'
              ? selectedSource.folder.path
              : selectedSource?.kind === 'remote'
                ? `${serverKindLabel(selectedSource.server.kind)} · ${selectedSource.server.userName}`
                : '从首页添加本地目录，或连接 Emby / Jellyfin'}
          </p>
        </div>
        {#if selectedSource?.kind === 'local'}
          <button class="secondary" onclick={rescanSelectedFolder}
            ><Icon name="refresh" />重新扫描</button
          >
        {:else if selectedSource?.kind === 'remote'}
          <button
            class="secondary"
            onclick={() => navigateRemoteCrumb(remoteCrumbs.length - 1)}
            ><Icon name="refresh" />刷新</button
          >
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
      {:else}
        <div class="library-toolbar">
          <div class="source-switcher" aria-label="媒体源">
            {#each folders as folder (folder.id)}
              <button
                class:active={selectedSource?.kind === 'local' &&
                  selectedSource.folder.id === folder.id}
                onclick={() => openLocalFolder(folder)}
              >
                <Icon name="folder" size={15} />{folder.name}<span
                  >{folder.mediaCount}</span
                >
              </button>
            {/each}
            {#each servers as server (server.id)}
              <button
                class:active={selectedSource?.kind === 'remote' &&
                  selectedSource.server.id === server.id}
                onclick={() => openRemoteServer(server)}
              >
                <Icon name="server" size={15} />{server.name}<span
                  >{serverKindLabel(server.kind)}</span
                >
              </button>
            {/each}
          </div>
          {#if !remoteDetail}
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
          {/if}
        </div>

        {#if selectedSource}
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
        {/if}

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
                      <h3>{selectedSeason?.name ?? '剧集'}</h3>
                    </div>
                    {#if remoteDetail.seasons.length > 1}
                      <div class="season-selector" aria-label="选择季度">
                        {#each remoteDetail.seasons as season (season.id)}
                          <button
                            class:active={season.id === selectedSeason?.id}
                            onclick={() => selectRemoteSeason(season.id)}
                          >
                            <span>{season.name}</span><small
                              >{seasonProgress(season)}</small
                            >
                          </button>
                        {/each}
                      </div>
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
          <span class="eyebrow">偏好设置</span>
          <h1>设置</h1>
          <p>配置播放器与家庭媒体服务器。</p>
        </div>
      </header>

      <section class="settings-card">
        <div class="settings-icon"><Icon name="play" size={22} /></div>
        <div class="settings-copy">
          <div class="settings-title">
            <h2>播放器</h2>
            <span class:available={player?.available}
              >{player?.available ? '可用' : '未找到'}</span
            >
          </div>
          <p>首页只会通过安全的参数数组启动这个程序，不会改写它的配置。</p>
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
          <div class="settings-actions">
            <button class="primary" onclick={choosePlayer}>选择播放器</button
            ><button class="secondary" onclick={resetPlayer}
              >恢复自动发现</button
            >
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
          <p>
            可使用用户名和密码登录，密码不会保存；服务器返回的令牌保存在本机应用数据目录，
            保存后不会再返回 WebView。
          </p>

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
              <label
                ><span>服务类型</span><select bind:value={serverDraft.kind}
                  ><option value="jellyfin">Jellyfin</option><option
                    value="emby">Emby</option
                  ></select
                ></label
              >
              <label
                ><span>显示名称（可选）</span><input
                  bind:value={serverDraft.name}
                  placeholder="例如：客厅媒体库"
                /></label
              >
              <label class="wide"
                ><span>服务器地址</span><input
                  bind:value={serverDraft.baseUrl}
                  type="url"
                  required
                  placeholder="http://192.168.1.20:8096"
                /><small>如配置了 Base URL，请包含完整路径。</small></label
              >
              <label class="wide"
                ><span>登录方式</span><select bind:value={serverDraft.authMode}
                  ><option value="password">用户名和密码</option><option
                    value="token">访问令牌 / API Key</option
                  ></select
                ></label
              >
              {#if serverDraft.authMode === 'password'}
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
                <p class="form-note wide">
                  密码只用于向服务器登录并换取访问令牌，不会保存到本机数据库；非可信网络请使用
                  HTTPS。
                </p>
              {:else}
                <label class="wide"
                  ><span>访问令牌 / API Key</span><input
                    bind:value={serverDraft.token}
                    type="password"
                    required
                    autocomplete="off"
                    placeholder="X-Emby-Token"
                  /></label
                >
                <label class="wide"
                  ><span>用户 ID（可选）</span><input
                    bind:value={serverDraft.userId}
                    autocomplete="off"
                    placeholder="API Key 对应多个用户时建议填写"
                  /></label
                >
              {/if}
              <div class="form-actions wide">
                <button
                  type="button"
                  class="secondary"
                  onclick={() => (showServerForm = false)}>取消</button
                ><button type="submit" class="primary">测试并保存</button>
              </div>
            </form>
          {:else}
            <div class="settings-actions">
              <button class="primary" onclick={() => (showServerForm = true)}
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
            <span>SQLite</span>
          </div>
          <p>
            数据库位于系统应用数据目录。移除目录只会删除索引，永远不会删除媒体文件。
          </p>
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
