import { defineConfig } from 'wxt';

// See https://wxt.dev/api/config.html
export default defineConfig({
  srcDir: '.',
  // Default is `.output`, hidden in macOS's Finder (and its file pickers) --
  // use a visible name so "load unpacked"/"load temporary add-on" dialogs
  // can actually navigate into it.
  outDir: 'dist',
  manifest: {
    name: 'Manga Web Translator (self-hosted)',
    description:
      'Detects manga/manhwa/webtoon panel images on any page and overlays a translated version, produced by your own local or cloud OCR+translation pipeline.',
    permissions: ['storage', 'activeTab', 'scripting', 'contextMenus'],
    // host_permissions is left open so the content script can run detection
    // on any reading site; the backend URL itself is configured by the user.
    host_permissions: ['<all_urls>'],
    commands: {
      'translate-page': {
        suggested_key: { default: 'Ctrl+Shift+T', mac: 'Command+Shift+T' },
        description: 'Translate detected images on the current page',
      },
    },
  },
});
