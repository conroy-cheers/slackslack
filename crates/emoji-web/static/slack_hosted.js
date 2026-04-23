const SESSION_KEY = 'slackslack.emojiWeb.session.v1';
const PKCE_KEY = 'slackslack.emojiWeb.pkce.v1';
const DEFAULT_REDIRECT = `${window.location.origin}${window.location.pathname}`;

function log(...args) {
  console.log('[ultramoji-viewer-4d]', ...args);
}

function readConfig() {
  const config = window.SLACK_EMOJI_APP_CONFIG ?? {};
  return {
    clientId: String(config.clientId ?? '').trim(),
    redirectUri: String(config.redirectUri ?? DEFAULT_REDIRECT).trim() || DEFAULT_REDIRECT,
    userScope: String(config.userScope ?? 'emoji:read').trim() || 'emoji:read',
  };
}

function loadSession() {
  try {
    const raw = localStorage.getItem(SESSION_KEY);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

function saveSession(session) {
  localStorage.setItem(SESSION_KEY, JSON.stringify(session));
}

function clearSession() {
  localStorage.removeItem(SESSION_KEY);
}

function loadPkce() {
  try {
    const raw = localStorage.getItem(PKCE_KEY);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

function savePkce(pkce) {
  localStorage.setItem(PKCE_KEY, JSON.stringify(pkce));
}

function clearPkce() {
  localStorage.removeItem(PKCE_KEY);
}

function pushUiState(wasm, {
  status,
  workspace = '',
  hint = '',
  signedIn = false,
  busy = false,
  loginEnabled = false,
}) {
  wasm.set_hosted_auth_state(status, workspace, hint, signedIn, busy, loginEnabled);
}

function applyUiState(state, fields) {
  state.ui = {
    status: '',
    workspace: '',
    hint: '',
    signedIn: false,
    busy: false,
    loginEnabled: false,
    ...state.ui,
    ...fields,
  };
  if (!state.ui.signedIn) {
    state.ui.workspace = '';
  }
  log('ui state', {
    status: state.ui.status,
    signedIn: state.ui.signedIn,
    busy: state.ui.busy,
    loginEnabled: state.ui.loginEnabled,
    workspace: state.ui.workspace,
  });
  pushUiState(state.wasm, state.ui);
}

function base64Url(bytes) {
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/g, '');
}

async function sha256(text) {
  const data = new TextEncoder().encode(text);
  const digest = await crypto.subtle.digest('SHA-256', data);
  return new Uint8Array(digest);
}

function randomString(byteLength = 32) {
  const bytes = new Uint8Array(byteLength);
  crypto.getRandomValues(bytes);
  return base64Url(bytes);
}

function oauthFormBody(fields) {
  const body = new URLSearchParams();
  for (const [key, value] of Object.entries(fields)) {
    if (value !== undefined && value !== null && value !== '') {
      body.set(key, String(value));
    }
  }
  return body;
}

async function postSlackForm(url, fields) {
  const response = await fetch(url, {
    method: 'POST',
    headers: {
      'content-type': 'application/x-www-form-urlencoded;charset=UTF-8',
    },
    body: oauthFormBody(fields),
  });
  const json = await response.json();
  if (!json.ok) {
    throw new Error(json.error || `Slack API error from ${url}`);
  }
  return json;
}

function extractTokenPayload(payload) {
  const authedUser = payload.authed_user ?? {};
  const accessToken = authedUser.access_token ?? payload.access_token ?? '';
  const refreshToken = authedUser.refresh_token ?? payload.refresh_token ?? '';
  const expiresIn = Number(authedUser.expires_in ?? payload.expires_in ?? 0);
  return {
    accessToken,
    refreshToken,
    expiresAt: expiresIn > 0 ? Date.now() + expiresIn * 1000 : null,
    scope: authedUser.scope ?? payload.scope ?? '',
    team: payload.team ?? null,
  };
}

async function buildPkceLoginUrl(config) {
  const state = randomString(24);
  const verifier = randomString(48);
  const challenge = base64Url(await sha256(verifier));
  savePkce({ state, verifier, redirectUri: config.redirectUri });
  const url = new URL('https://slack.com/oauth/v2/authorize');
  url.searchParams.set('client_id', config.clientId);
  url.searchParams.set('redirect_uri', config.redirectUri);
  url.searchParams.set('user_scope', config.userScope);
  url.searchParams.set('state', state);
  url.searchParams.set('code_challenge', challenge);
  url.searchParams.set('code_challenge_method', 'S256');
  return url.toString();
}

async function maybeHandleOAuthCallback(config) {
  const url = new URL(window.location.href);
  const code = url.searchParams.get('code');
  const state = url.searchParams.get('state');
  const error = url.searchParams.get('error');
  if (error) {
    url.searchParams.delete('error');
    url.searchParams.delete('error_description');
    url.searchParams.delete('state');
    url.searchParams.delete('code');
    history.replaceState({}, '', url);
    throw new Error(`Slack authorization failed: ${error}`);
  }
  if (!code) {
    return null;
  }
  log('handling oauth callback');

  url.searchParams.delete('code');
  url.searchParams.delete('state');
  history.replaceState({}, '', url);

  const pkce = loadPkce();
  if (!pkce || pkce.state !== state) {
    clearPkce();
    log('ignoring stale oauth callback due to state mismatch');
    return null;
  }

  const response = await postSlackForm('https://slack.com/api/oauth.v2.access', {
    client_id: config.clientId,
    code,
    redirect_uri: pkce.redirectUri || config.redirectUri,
    code_verifier: pkce.verifier,
  });
  clearPkce();

  const session = {
    ...extractTokenPayload(response),
    installedAt: Date.now(),
  };
  if (!session.accessToken) {
    throw new Error('Slack OAuth response did not include a user access token');
  }
  log('oauth callback complete', {
    team: session.team?.name ?? '',
    hasRefresh: Boolean(session.refreshToken),
    hasExpiry: Boolean(session.expiresAt),
  });
  saveSession(session);
  return session;
}

async function ensureFreshSession(config, session) {
  if (!session?.refreshToken || !session?.expiresAt) {
    log('session does not require refresh');
    return session;
  }
  if (session.expiresAt - Date.now() > 60_000) {
    log('session still fresh');
    return session;
  }
  log('refreshing slack session');

  const response = await postSlackForm('https://slack.com/api/oauth.v2.access', {
    client_id: config.clientId,
    grant_type: 'refresh_token',
    refresh_token: session.refreshToken,
  });
  const refreshed = {
    ...session,
    ...extractTokenPayload(response),
    refreshedAt: Date.now(),
  };
  if (!refreshed.accessToken) {
    throw new Error('Slack token refresh did not return an access token');
  }
  log('session refresh complete', {
    team: refreshed.team?.name ?? session.team?.name ?? '',
  });
  saveSession(refreshed);
  return refreshed;
}

function resolveEmojiCatalog(rawEmoji) {
  const resolvedUrl = new Map();
  const resolutionState = new Set();

  function resolveValue(name) {
    if (resolvedUrl.has(name)) {
      return resolvedUrl.get(name);
    }
    if (resolutionState.has(name)) {
      return null;
    }
    resolutionState.add(name);
    const value = rawEmoji?.[name];
    let resolved = null;
    if (typeof value === 'string') {
      if (value.startsWith('alias:')) {
        resolved = resolveValue(value.slice(6).trim());
      } else if (/^https?:\/\//i.test(value)) {
        resolved = value;
      }
    } else if (value && typeof value.url === 'string' && /^https?:\/\//i.test(value.url)) {
      resolved = value.url;
    }
    resolutionState.delete(name);
    if (resolved) {
      resolvedUrl.set(name, resolved);
    }
    return resolved;
  }

  const names = Object.keys(rawEmoji || {})
    .filter((name) => Boolean(resolveValue(name)))
    .sort((a, b) => a.localeCompare(b));

  return { names, assetUrls: resolvedUrl };
}

async function fetchEmojiCatalog(session) {
  log('fetching emoji catalog', { team: session.team?.name ?? '' });
  const payload = await postSlackForm('https://slack.com/api/emoji.list', {
    token: session.accessToken,
    include_categories: 'true',
  });
  return resolveEmojiCatalog(payload.emoji);
}

async function fetchEmojiBytes(url) {
  const relayUrl = new URL('/emoji-asset', window.location.origin);
  relayUrl.searchParams.set('url', url);
  log('fetching emoji bytes', { relayUrl: relayUrl.toString(), sourceUrl: url });
  const response = await fetch(relayUrl, { method: 'GET', credentials: 'same-origin' });
  if (!response.ok) {
    throw new Error(`Emoji fetch failed: ${response.status} ${response.statusText}`);
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  log('emoji bytes fetched', { sourceUrl: url, byteLength: bytes.byteLength });
  return bytes;
}

function installStorageSync(state) {
  window.addEventListener('storage', async (event) => {
    if (event.key !== SESSION_KEY) {
      return;
    }
    state.session = loadSession();
    if (state.session) {
      try {
        state.session = await ensureFreshSession(state.config, state.session);
        await syncCatalog(state);
      } catch (error) {
        clearSession();
        state.session = null;
        state.assetUrls = new Map();
        state.assetCache.clear();
        state.currentEmojiName = '';
        state.currentRequestId += 1;
        state.wasm.set_gallery_entries('');
        state.wasm.clear_active_emoji_texture();
        applyUiState(state, {
          status: 'SLACK SESSION FAILED',
          hint: String(error.message || error),
          signedIn: false,
          busy: false,
          loginEnabled: Boolean(state.config.clientId),
        });
      }
    } else {
      state.assetUrls = new Map();
      state.assetCache.clear();
      state.currentEmojiName = '';
      state.currentRequestId += 1;
      state.wasm.set_gallery_entries('');
      state.wasm.clear_active_emoji_texture();
      applyUiState(state, {
        status: 'SLACK SIGN-IN REQUIRED',
        hint: state.config.clientId
          ? 'Press ENTER to open Slack login in a new tab.'
          : 'Set window.SLACK_EMOJI_APP_CONFIG.clientId before using hosted auth.',
        signedIn: false,
        busy: false,
        loginEnabled: Boolean(state.config.clientId),
      });
    }
  });
}

export async function bootHostedEmojiApp(wasm) {
  const config = readConfig();
  const state = {
    wasm,
    config,
    session: loadSession(),
    assetUrls: new Map(),
    assetCache: new Map(),
    currentEmojiName: '',
    loadedEmojiName: '',
    currentRequestId: 0,
    signOutRequestSeen: 0,
    ui: {
      status: '',
      workspace: '',
      hint: '',
      signedIn: Boolean(loadSession()),
      busy: false,
      loginEnabled: Boolean(config.clientId),
    },
  };
  log('boot hosted app', {
    hasClientId: Boolean(config.clientId),
    hasSession: Boolean(state.session),
  });

  const openLoginTab = async () => {
    let popup = null;
    try {
      if (!config.clientId) {
        applyUiState(state, {
          status: 'LOGIN NOT CONFIGURED',
          hint: 'Set window.SLACK_EMOJI_APP_CONFIG.clientId before using hosted auth.',
          signedIn: false,
          busy: false,
          loginEnabled: false,
        });
        return;
      }
      popup = window.open('', '_blank');
      if (!popup) {
        applyUiState(state, {
          status: 'POPUP BLOCKED',
          hint: 'Allow popups for this site, then press ENTER again.',
          signedIn: Boolean(state.session),
          workspace: state.session?.team?.name || '',
          busy: false,
          loginEnabled: true,
        });
        return;
      }
      try {
        popup.document.write(`<!doctype html><html><head><title>Slack Login</title><style>html,body{height:100%;margin:0}body{display:flex;align-items:center;justify-content:center;background:#0c121c;color:#d6e8ff;font:16px monospace}</style></head><body>Connecting to Slack...</body></html>`);
        popup.document.close();
      } catch {}
      const loginUrl = await buildPkceLoginUrl(config);
      log('opening slack login tab', { loginUrl });
      popup.location.href = loginUrl;
      applyUiState(state, {
        status: 'OPENED SLACK LOGIN',
        hint: 'Complete sign-in in the new tab. This window will update automatically.',
        signedIn: Boolean(state.session),
        workspace: state.session?.team?.name || '',
        busy: false,
        loginEnabled: true,
      });
    } catch (error) {
      try {
        popup?.close();
      } catch {}
      applyUiState(state, {
        status: 'UNABLE TO START SLACK SIGN-IN',
        hint: String(error.message || error),
        signedIn: Boolean(state.session),
        workspace: state.session?.team?.name || '',
        busy: false,
        loginEnabled: Boolean(config.clientId),
      });
    }
  };

  const signOut = () => {
    log('signing out');
    clearSession();
    clearPkce();
    state.session = null;
    state.assetUrls = new Map();
    state.assetCache.clear();
    state.currentEmojiName = '';
    state.loadedEmojiName = '';
    state.currentRequestId += 1;
    state.wasm.set_gallery_entries('');
    state.wasm.clear_active_emoji_texture();
    applyUiState(state, {
      status: 'SLACK SIGN-IN REQUIRED',
      hint: config.clientId
        ? 'Press ENTER to open Slack login in a new tab.'
        : 'Set window.SLACK_EMOJI_APP_CONFIG.clientId before using hosted auth.',
      signedIn: false,
      busy: false,
      loginEnabled: Boolean(config.clientId),
    });
  };

  window.addEventListener('keydown', (event) => {
    if (event.key !== 'Enter') {
      return;
    }
    log('keydown', {
      key: event.key,
      signedIn: state.ui.signedIn,
      busy: state.ui.busy,
      loginEnabled: state.ui.loginEnabled,
    });
    if (state.ui.signedIn || state.ui.busy || !state.ui.loginEnabled) {
      return;
    }
    event.preventDefault();
    void openLoginTab();
  });

  installStorageSync(state);

  try {
    applyUiState(state, {
      status: 'INITIALIZING',
      hint: 'Preparing hosted Slack session...',
      signedIn: Boolean(state.session),
      busy: true,
      loginEnabled: Boolean(config.clientId),
    });

    if (config.clientId) {
      const callbackSession = await maybeHandleOAuthCallback(config);
      if (callbackSession) {
        state.session = callbackSession;
        if (window.opener && window.opener !== window) {
          applyUiState(state, {
            status: 'LOGIN COMPLETE',
            workspace: callbackSession.team?.name || '',
            hint: 'Return to the original tab. This tab will close if the browser allows it.',
            signedIn: true,
            busy: false,
            loginEnabled: true,
          });
          setTimeout(() => window.close(), 150);
          return;
        }
      }
    }

    if (state.session) {
      state.session = await ensureFreshSession(config, state.session);
      log('session ready', { team: state.session?.team?.name ?? '' });
      await syncCatalog(state);
    } else {
      log('no session available after boot');
      wasm.set_gallery_entries('');
      applyUiState(state, {
        status: 'SLACK SIGN-IN REQUIRED',
        hint: config.clientId
          ? 'Press ENTER to open Slack login in a new tab.'
          : 'Set window.SLACK_EMOJI_APP_CONFIG.clientId before using hosted auth.',
        signedIn: false,
        busy: false,
        loginEnabled: Boolean(config.clientId),
      });
    }
  } catch (error) {
    clearSession();
    state.session = null;
    wasm.set_gallery_entries('');
    wasm.clear_active_emoji_texture();
    applyUiState(state, {
      status: 'SLACK SESSION FAILED',
      hint: String(error.message || error),
      signedIn: false,
      busy: false,
      loginEnabled: Boolean(config.clientId),
    });
  }

  const tick = async () => {
    try {
      const signOutNonce = wasm.sign_out_request_nonce();
      if (signOutNonce !== state.signOutRequestSeen) {
        state.signOutRequestSeen = signOutNonce;
        if (state.session) {
          signOut();
        }
      }
      if (!state.session) {
        if (state.currentEmojiName) {
          state.currentEmojiName = '';
        }
        if (state.loadedEmojiName) {
          state.loadedEmojiName = '';
        }
        window.requestAnimationFrame(() => {
          void tick();
        });
        return;
      }
      const name = wasm.current_emoji_name();
      if (name !== state.currentEmojiName) {
        log('current emoji changed', { from: state.currentEmojiName, to: name });
        state.currentEmojiName = name;
      }
      if (name !== state.loadedEmojiName) {
        log('emoji asset out of sync', { selected: name, loaded: state.loadedEmojiName });
        await ensureEmojiTexture(state, name);
      }
    } catch (error) {
      console.error('[ultramoji-viewer-4d] tick failed', error);
      if (state.session) {
        applyUiState(state, {
          status: 'EMOJI PREVIEW FETCH FAILED',
          workspace: state.session?.team?.name || '',
          hint: String(error.message || error),
          signedIn: true,
          busy: false,
          loginEnabled: Boolean(config.clientId),
        });
      }
    }
    window.requestAnimationFrame(() => {
      void tick();
    });
  };
  window.requestAnimationFrame(() => {
    void tick();
  });
}

async function syncCatalog(state) {
  if (!state.session) {
    throw new Error('No Slack session');
  }
  applyUiState(state, {
    status: 'LOADING WORKSPACE EMOJI',
    workspace: state.session?.team?.name || '',
    hint: 'Fetching emoji.list from Slack.',
    signedIn: true,
    busy: true,
    loginEnabled: Boolean(state.config.clientId),
  });
  const catalog = await fetchEmojiCatalog(state.session);
  log('emoji catalog loaded', { count: catalog.names.length });
  state.assetUrls = catalog.assetUrls;
  state.assetCache.clear();
  state.currentEmojiName = '';
  state.loadedEmojiName = '';
  state.currentRequestId += 1;
  state.wasm.set_gallery_entries(catalog.names.join('\n'));
  state.wasm.clear_active_emoji_texture();
  applyUiState(state, {
    status: `LOADED ${catalog.names.length} EMOJI`,
    workspace: state.session?.team?.name || '',
    hint: '',
    signedIn: true,
    busy: false,
    loginEnabled: Boolean(state.config.clientId),
  });
}

async function ensureEmojiTexture(state, name) {
  if (!state.session) {
    log('skipping emoji texture load without active session', { name });
    return;
  }
  const requestId = ++state.currentRequestId;
  if (!name) {
    log('clearing emoji texture because name is empty');
    state.wasm.clear_active_emoji_texture();
    state.loadedEmojiName = '';
    return;
  }
  const url = state.assetUrls.get(name);
  if (!url) {
    log('no asset url for emoji', { name });
    state.wasm.clear_active_emoji_texture();
    state.loadedEmojiName = '';
    return;
  }
  if (state.assetCache.has(url)) {
    log('using cached emoji bytes', { name, url });
    if (requestId === state.currentRequestId) {
      const ok = state.wasm.set_active_emoji_texture_bytes(name, state.assetCache.get(url));
      log('cached emoji decode handoff', { name, ok });
      if (ok) {
        state.loadedEmojiName = name;
      }
    }
    return;
  }

  applyUiState(state, {
    workspace: state.session?.team?.name || '',
    hint: `Fetching preview bytes via same-origin relay for ${url}`,
    signedIn: Boolean(state.session),
    busy: false,
    loginEnabled: Boolean(state.config.clientId),
  });
  const bytes = await fetchEmojiBytes(url);
  state.assetCache.set(url, bytes);
  if (requestId !== state.currentRequestId) {
    log('discarding stale emoji response', { name, url, requestId, currentRequestId: state.currentRequestId });
    return;
  }
  const decoded = state.wasm.set_active_emoji_texture_bytes(name, bytes);
  log('emoji decode handoff', { name, url, decoded });
  if (!decoded) {
    throw new Error(`Could not decode preview image for :${name}:`);
  }
  state.loadedEmojiName = name;
  applyUiState(state, {
    workspace: state.session?.team?.name || '',
    hint: 'Preview ready.',
    signedIn: Boolean(state.session),
    busy: false,
    loginEnabled: Boolean(state.config.clientId),
  });
}
