<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { onMount } from 'svelte';
  import { api } from './api';
  import Icon from './components/Icon.svelte';
  import type { FolderSummary, MediaItem, PlayerStatus } from './types';

  type View = 'home' | 'library' | 'settings';
  type Toast = { kind: 'success' | 'error'; message: string };

  let view = $state<View>('home');
  let folders = $state<FolderSummary[]>([]);
  let recentMedia = $state<MediaItem[]>([]);
  let media = $state<MediaItem[]>([]);
  let selectedFolder = $state<FolderSummary | null>(null);
  let player = $state<PlayerStatus | null>(null);
  let search = $state('');
  let busy = $state(false);
  let busyMessage = $state('正在整理媒体库…');
  let toast = $state<Toast | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  const totalMedia = $derived(
    folders.reduce((total, folder) => total + folder.mediaCount, 0),
  );
  const visibleMedia = $derived(
    search.trim()
      ? media.filter((item) =>
          item.name
            .toLocaleLowerCase()
            .includes(search.trim().toLocaleLowerCase()),
        )
      : media,
  );

  onMount(async () => {
    await refreshOverview();
  });

  async function refreshOverview() {
    try {
      [folders, recentMedia, player] = await Promise.all([
        api.listFolders(),
        api.listMedia(undefined, undefined, 12),
        api.getPlayerStatus(),
      ]);
      if (selectedFolder) {
        selectedFolder =
          folders.find((folder) => folder.id === selectedFolder?.id) ?? null;
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

  async function openFolder(folder: FolderSummary) {
    view = 'library';
    selectedFolder = folder;
    search = '';
    try {
      media = await api.listMedia(folder.id, undefined, 2000);
    } catch (error) {
      notify('error', normalizeError(error));
    }
  }

  async function rescanFolder(folder: FolderSummary) {
    await withBusy(`正在重新扫描“${folder.name}”…`, async () => {
      const updated = await api.rescanFolder(folder.id);
      await refreshOverview();
      if (selectedFolder?.id === folder.id) {
        selectedFolder = updated;
        media = await api.listMedia(folder.id, undefined, 2000);
      }
      notify('success', `扫描完成，共 ${updated.mediaCount} 个视频`);
    });
  }

  async function removeFolder(folder: FolderSummary) {
    if (!window.confirm(`从首页移除“${folder.name}”？媒体文件不会被删除。`))
      return;
    try {
      await api.removeFolder(folder.id);
      if (selectedFolder?.id === folder.id) {
        selectedFolder = null;
        media = [];
        view = 'home';
      }
      await refreshOverview();
      notify('success', '已移除媒体目录，原文件没有变化');
    } catch (error) {
      notify('error', normalizeError(error));
    }
  }

  async function play(item: MediaItem) {
    try {
      await api.playMedia(item.id);
      notify('success', `已交给 mpv 播放：${item.name}`);
    } catch (error) {
      notify('error', normalizeError(error));
      if (!player?.available) view = 'settings';
    }
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
</script>

<svelte:head>
  <title>mpv-enjoy Home</title>
</svelte:head>

<div class="app-shell">
  <aside class="sidebar">
    <div class="brand">
      <div class="brand-mark"><Icon name="play" size={18} /></div>
      <div>
        <strong>mpv-enjoy</strong>
        <span>Home</span>
      </div>
    </div>

    <nav aria-label="主导航">
      <button class:active={view === 'home'} onclick={() => (view = 'home')}>
        <Icon name="home" />
        <span>首页</span>
      </button>
      <button
        class:active={view === 'library'}
        onclick={() => {
          view = 'library';
          if (!selectedFolder && folders[0]) void openFolder(folders[0]);
        }}
      >
        <Icon name="library" />
        <span>媒体库</span>
        {#if totalMedia > 0}<em>{totalMedia}</em>{/if}
      </button>
      <button
        class:active={view === 'settings'}
        onclick={() => (view = 'settings')}
      >
        <Icon name="settings" />
        <span>设置</span>
        <i
          class:ready={player?.available}
          aria-label={player?.available ? '播放器可用' : '播放器未配置'}
        ></i>
      </button>
    </nav>

    <div class="sidebar-note">
      <Icon name="sparkles" size={18} />
      <p>首页只负责整理和启动，播放体验完整交给你的 mpv。</p>
    </div>
    <span class="version">Technical preview · 0.1.0</span>
  </aside>

  <main>
    {#if view === 'home'}
      <header class="page-header">
        <div>
          <span class="eyebrow">欢迎回来</span>
          <h1>今天想看点什么？</h1>
          <p>从熟悉的文件夹出发，用你喜欢的 mpv 播放。</p>
        </div>
        <button class="primary" onclick={addFolder}>
          <Icon name="add" />
          添加媒体目录
        </button>
      </header>

      <section class="hero">
        <div class="hero-copy">
          <span class="hero-label"
            ><Icon name="sparkles" size={16} /> 本地、安静、直接</span
          >
          <h2>你的媒体，随手就播。</h2>
          <p>不上传文件，不改变目录结构，也不接管播放器配置。</p>
          <div class="hero-stats">
            <div><strong>{folders.length}</strong><span>个目录</span></div>
            <div><strong>{totalMedia}</strong><span>个视频</span></div>
            <div>
              <strong class:status-good={player?.available}
                >{player?.available ? '就绪' : '待设置'}</strong
              >
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
            <h2>媒体目录</h2>
            <p>你添加到首页的本地收藏</p>
          </div>
          {#if folders.length > 0}<button
              class="text-button"
              onclick={addFolder}>添加目录 <Icon name="add" size={16} /></button
            >{/if}
        </div>

        {#if folders.length === 0}
          <div class="empty-state">
            <div class="empty-icon"><Icon name="folder" size={30} /></div>
            <h3>从一个媒体目录开始</h3>
            <p>选择电影、剧集或动画所在的文件夹，首页会在本机建立索引。</p>
            <button class="secondary" onclick={addFolder}
              ><Icon name="add" /> 选择目录</button
            >
          </div>
        {:else}
          <div class="folder-grid">
            {#each folders as folder (folder.id)}
              <article class="folder-card">
                <button class="folder-main" onclick={() => openFolder(folder)}>
                  <span class="folder-cover"
                    ><Icon name="folder" size={34} /></span
                  >
                  <span class="folder-copy">
                    <strong>{folder.name}</strong>
                    <small title={folder.path}>{folder.path}</small>
                    <span
                      >{folder.mediaCount} 个视频 · {formatScanTime(
                        folder.lastScannedAt,
                      )}</span
                    >
                  </span>
                  <span class="round-play"><Icon name="play" size={15} /></span>
                </button>
                <div class="folder-actions">
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
          </div>
        {/if}
      </section>

      {#if recentMedia.length > 0}
        <section class="content-section">
          <div class="section-heading">
            <div>
              <h2>最近更新</h2>
              <p>按文件更新时间排列</p>
            </div>
          </div>
          <div class="media-row">
            {#each recentMedia.slice(0, 6) as item (item.id)}
              <button
                class="media-poster"
                onclick={() => play(item)}
                title={`播放 ${item.name}`}
              >
                <span class="poster-art"
                  ><Icon name="video" size={30} /><em>{item.extension}</em><i
                    ><Icon name="play" size={16} /></i
                  ></span
                >
                <strong>{item.name}</strong>
              </button>
            {/each}
          </div>
        </section>
      {/if}
    {:else if view === 'library'}
      <header class="page-header compact">
        <div>
          <span class="eyebrow">媒体库</span>
          <h1>{selectedFolder?.name ?? '选择一个目录'}</h1>
          <p title={selectedFolder?.path}>
            {selectedFolder?.path ?? '从左侧或首页打开媒体目录'}
          </p>
        </div>
        {#if selectedFolder}
          <button
            class="secondary"
            onclick={() => rescanFolder(selectedFolder!)}
            ><Icon name="refresh" /> 重新扫描</button
          >
        {/if}
      </header>

      {#if folders.length === 0}
        <div class="empty-state library-empty">
          <div class="empty-icon"><Icon name="folder" size={30} /></div>
          <h3>媒体库还是空的</h3>
          <p>先添加一个本地媒体目录。</p>
          <button class="primary" onclick={addFolder}
            ><Icon name="add" /> 添加目录</button
          >
        </div>
      {:else}
        <div class="library-toolbar">
          <div class="folder-switcher">
            {#each folders as folder (folder.id)}
              <button
                class:active={selectedFolder?.id === folder.id}
                onclick={() => openFolder(folder)}
                >{folder.name}<span>{folder.mediaCount}</span></button
              >
            {/each}
          </div>
          <label class="search-box">
            <Icon name="search" size={18} />
            <input
              bind:value={search}
              type="search"
              placeholder="搜索当前目录"
              aria-label="搜索当前目录"
            />
            {#if search}<button
                onclick={() => (search = '')}
                aria-label="清除搜索">×</button
              >{/if}
          </label>
        </div>

        {#if selectedFolder && visibleMedia.length > 0}
          <div class="media-list">
            {#each visibleMedia as item, index (item.id)}
              <button class="media-list-item" onclick={() => play(item)}>
                <span class="list-index"
                  >{String(index + 1).padStart(2, '0')}</span
                >
                <span class="list-thumb"
                  ><Icon name="video" /><i><Icon name="play" size={13} /></i
                  ></span
                >
                <span class="list-copy"
                  ><strong>{item.name}</strong><small title={item.path}
                    >{item.path}</small
                  ></span
                >
                <em>{item.extension.toUpperCase()}</em>
                <span class="list-play"><Icon name="play" size={14} /></span>
              </button>
            {/each}
          </div>
        {:else if selectedFolder}
          <div class="empty-state library-empty">
            <div class="empty-icon"><Icon name="search" size={28} /></div>
            <h3>{search ? '没有匹配的媒体' : '没有找到支持的视频'}</h3>
            <p>
              {search
                ? '换个关键词试试。'
                : '可重新扫描，或检查文件扩展名是否受支持。'}
            </p>
          </div>
        {/if}
      {/if}
    {:else}
      <header class="page-header compact">
        <div>
          <span class="eyebrow">偏好设置</span>
          <h1>设置</h1>
          <p>选择实际承担播放任务的 mpv。</p>
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
            <button class="primary" onclick={choosePlayer}>选择播放器</button>
            <button class="secondary" onclick={resetPlayer}>恢复自动发现</button
            >
          </div>
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
    <strong>{busyMessage}</strong>
    <span>大目录可能需要一点时间</span>
  </div>
{/if}

{#if toast}
  <div
    class:error={toast.kind === 'error'}
    class="toast"
    role="status"
    aria-live="polite"
  >
    <span>{toast.kind === 'success' ? '✓' : '!'}</span>
    {toast.message}
  </div>
{/if}
