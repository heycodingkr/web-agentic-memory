import { error } from '@sveltejs/kit';
import type { EntryGenerator, RequestHandler } from './$types';
import calendar from '../../../../../docs/test/benchmarks/google-calendar.md?raw';
import notion from '../../../../../docs/test/benchmarks/notion.md?raw';
import canva from '../../../../../docs/test/benchmarks/canva.md?raw';
import tripFlights from '../../../../../docs/test/benchmarks/trip-flights.md?raw';

// Serve the documentation source itself; landing links work in the static build
// without maintaining another copy or depending on a published GitHub branch.
const reports: Record<string, string> = {
	'google-calendar.md': calendar,
	'notion.md': notion,
	'canva.md': canva,
	'trip-flights.md': tripFlights
};

export const prerender = true;
export const entries: EntryGenerator = () => Object.keys(reports).map((report) => ({ report }));

export const GET: RequestHandler = ({ params }) => {
	const report = Object.hasOwn(reports, params.report) ? reports[params.report] : undefined;
	if (!report) error(404, 'Benchmark report not found');
	return new Response(report, {
		headers: { 'Content-Type': 'text/plain; charset=utf-8' }
	});
};
