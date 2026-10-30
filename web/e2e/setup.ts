import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';

export default async function setup() {
	execFileSync('cargo', ['build', '--locked', '-p', 'wam-cli', '-p', 'wam-mcp'], {
		cwd: resolve(process.cwd(), '..'),
		stdio: 'inherit'
	});
}
