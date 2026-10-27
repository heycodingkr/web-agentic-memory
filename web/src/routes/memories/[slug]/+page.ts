import type { PageLoad } from './$types';

export const entries = () => [{ slug: 'web:example-draft' }];

export const load: PageLoad = ({ params }) => ({
	memoryId: params.slug
});
