// Shared theme handling for the launcher's own windows (splash + settings).
// Applies the persisted theme preference to <html data-theme> and keeps it in
// sync with the OS appearance when the preference is "system".
(function () {
  'use strict';

  var mq = window.matchMedia('(prefers-color-scheme: dark)');
  var pref = 'system';

  function apply(nextPref) {
    if (nextPref) pref = nextPref;
    window.__themePref = pref;
    var resolved = pref === 'system' ? (mq.matches ? 'dark' : 'light') : pref;
    document.documentElement.dataset.theme = resolved;
  }
  window.__applyTheme = apply;

  mq.addEventListener('change', function () {
    // Only system mode reacts to live OS appearance changes.
    if (window.__themePref === 'system') apply('system');
  });

  // Wire up IPC when available (withGlobalTauri injects __TAURI__ at document
  // start; fall back to DOMContentLoaded in case it is injected later).
  function wire() {
    if (!window.__TAURI__) return false;
    window.__TAURI__.core
      .invoke('get_settings')
      .then(function (settings) {
        apply(settings.theme);
      })
      .catch(function () {
        apply('system');
      });
    window.__TAURI__.event.listen('settings-changed', function (event) {
      apply(event.payload.theme);
    });
    return true;
  }
  if (!wire()) {
    document.addEventListener('DOMContentLoaded', function () {
      if (!wire()) apply('system');
    });
  }
})();
