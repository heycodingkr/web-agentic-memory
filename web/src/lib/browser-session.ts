import { get, writable } from 'svelte/store';
import { api, ApiError, setCsrfToken, type AuthSession } from './api';

export const browserSession = writable<AuthSession | null>(null);
let generation = 0;
let pending: Promise<AuthSession | null> | null = null;
let loaded = false;
export function setBrowserSession(value: AuthSession | null) {
  generation++; pending = null; loaded = true;
  setCsrfToken(value?.csrf_token); browserSession.set(value);
}
export function loadBrowserSession(force = false): Promise<AuthSession | null> {
  if (pending && !force) return pending;
  if (loaded && !force) return Promise.resolve(get(browserSession));
  const requestGeneration = ++generation;
  const request = api<AuthSession>('/v1/auth/session').then(value => {
    if (requestGeneration !== generation) return get(browserSession);
    loaded = true; setCsrfToken(value.csrf_token); browserSession.set(value); return value;
  }).catch(cause => {
    if (requestGeneration !== generation) return get(browserSession);
    if (cause instanceof ApiError && cause.status === 401) { loaded = true; setCsrfToken(); browserSession.set(null); return null; }
    throw cause;
  }).finally(() => { if (pending === request) pending = null; });
  pending = request;
  return request;
}
