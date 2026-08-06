<script lang="ts">
  type IconName =
    | 'add'
    | 'back'
    | 'folder'
    | 'home'
    | 'library'
    | 'more'
    | 'play'
    | 'refresh'
    | 'search'
    | 'server'
    | 'settings'
    | 'sparkles'
    | 'trash'
    | 'video';

  let { name, size = 20 }: { name: IconName; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if name === 'home'}
    <path d="m3 10 9-7 9 7v9a2 2 0 0 1-2 2h-5v-7h-4v7H5a2 2 0 0 1-2-2Z" />
  {:else if name === 'library'}
    <path
      d="M4 5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2Z"
    />
    <path d="M8 3v18M12 8h4M12 12h4" />
  {:else if name === 'settings'}
    <circle cx="12" cy="12" r="3" />
    <path
      d="M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06-2.83 2.83-.06-.06a1.7 1.7 0 0 0-1.88-.34 1.7 1.7 0 0 0-1.03 1.56V21h-4v-.09A1.7 1.7 0 0 0 8.96 19.4a1.7 1.7 0 0 0-1.88.34l-.06.06-2.83-2.83.06-.06A1.7 1.7 0 0 0 4.6 15a1.7 1.7 0 0 0-1.56-1.03H3v-4h.09A1.7 1.7 0 0 0 4.6 8.96a1.7 1.7 0 0 0-.34-1.88l-.06-.06 2.83-2.83.06.06A1.7 1.7 0 0 0 9 4.6a1.7 1.7 0 0 0 1-1.56V3h4v.09A1.7 1.7 0 0 0 15.04 4.6a1.7 1.7 0 0 0 1.88-.34l.06-.06 2.83 2.83-.06.06A1.7 1.7 0 0 0 19.4 9c.12.62.66 1.06 1.29 1.06H21v4h-.09A1.7 1.7 0 0 0 19.4 15Z"
    />
  {:else if name === 'folder'}
    <path
      d="M3 7a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z"
    />
  {:else if name === 'video'}
    <rect x="3" y="5" width="14" height="14" rx="2" />
    <path d="m17 10 4-2v8l-4-2Z" />
  {:else if name === 'server'}
    <rect x="4" y="3" width="16" height="7" rx="2" />
    <rect x="4" y="14" width="16" height="7" rx="2" />
    <path d="M8 6.5h.01M8 17.5h.01M12 6.5h5M12 17.5h5" />
  {:else if name === 'play'}
    <path d="m8 5 11 7-11 7Z" fill="currentColor" stroke="none" />
  {:else if name === 'search'}
    <circle cx="11" cy="11" r="7" />
    <path d="m20 20-4-4" />
  {:else if name === 'add'}
    <path d="M12 5v14M5 12h14" />
  {:else if name === 'refresh'}
    <path d="M20 6v5h-5M4 18v-5h5" />
    <path d="M18 9a7 7 0 0 0-12-2L4 11M6 15a7 7 0 0 0 12 2l2-4" />
  {:else if name === 'trash'}
    <path d="M4 7h16M9 7V4h6v3M7 7l1 14h8l1-14M10 11v6M14 11v6" />
  {:else if name === 'back'}
    <path d="m15 18-6-6 6-6" />
  {:else if name === 'sparkles'}
    <path
      d="m12 3 1.2 3.8L17 8l-3.8 1.2L12 13l-1.2-3.8L7 8l3.8-1.2ZM19 14l.7 2.3L22 17l-2.3.7L19 20l-.7-2.3L16 17l2.3-.7Z"
    />
  {:else if name === 'more'}
    <circle cx="5" cy="12" r="1" fill="currentColor" stroke="none" />
    <circle cx="12" cy="12" r="1" fill="currentColor" stroke="none" />
    <circle cx="19" cy="12" r="1" fill="currentColor" stroke="none" />
  {/if}
</svg>
