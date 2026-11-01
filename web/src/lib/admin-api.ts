export interface AdminSession { csrf_token: string; expires_at: number }
export class AdminError extends Error {
    constructor(public status: number) {
        super(status === 401 ? 'Your sign-in session has expired. Please sign in again.'
            : status === 403 ? 'Administrator access could not be verified. Try again from the administrator console.'
            : status === 429 ? 'Too many requests. Try again shortly.'
            : status === 409 ? 'This item changed during another review. Refresh and try again.'
            : status === 400 ? 'Check the administrator email address and try again.'
            : "We couldn't complete the request. Try again shortly.");
    }
}
// Admin credentials and link tokens stay in memory/cookies and never enter customer telemetry.
export async function adminApi<T>(path: string, init: RequestInit = {}, csrf?: string): Promise<T> {
    const headers = new Headers(init.headers);
    headers.set('content-type', 'application/json');
    if (csrf) headers.set('x-csrf-token', csrf);
    const response = await fetch(path, { ...init, headers, credentials: 'same-origin', cache: 'no-store', redirect: 'error' });
    if (!response.ok) throw new AdminError(response.status);
    return response.status === 204 ? undefined as T : await response.json() as T;
}
