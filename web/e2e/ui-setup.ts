import { basename, join } from 'node:path';
import { existsSync, readFileSync, rmSync } from 'node:fs';

export default function setup() {
	const home = process.env.WAM_UI_FIREFOX_HOME;
	if (!home) return;
	if (!basename(home).startsWith('wam-ui-firefox-') || readFileSync(join(home, '.wam-ui-owned'), 'utf8') !== 'wam-ui') {
		throw new Error('Refusing an unowned Firefox fixture home');
	}
	// Returning a teardown makes cleanup errors fail the suite itself, before
	// the reporter declares completion. The config's exit handler is a fallback.
	return () => {
		rmSync(home, { recursive: true, force: true });
		if (existsSync(home)) throw new Error('Owned Firefox fixture home remains');
	};
}
