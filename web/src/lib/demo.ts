export type Memory = {
	slug: string;
	site: string;
	domain: string;
	title: string;
	category: string;
	summary: string;
	verified: string;
	version: string;
	contributors: number;
	hint: string;
	steps: string[];
	caveat: string;
};

export const memories: Memory[] = [
	{
		slug: 'medium-draft',
		site: 'Medium',
		domain: 'medium.com',
		title: 'Save a clean draft',
		category: 'Blog writing',
		summary: 'Add a title and body, then save a draft without creating empty paragraphs.',
		verified: 'Demo data',
		version: 'v3',
		contributors: 8,
		hint: 'In the Medium editor, add the title and body separately. Use one Enter between paragraphs, review the structure in Preview, and save without publishing.',
		steps: ['Select the title field.', 'Write the body one paragraph at a time, using one Enter between paragraphs.', 'Review the title and paragraph structure in Preview, then choose Save draft.'],
		caveat: 'This catalog item is a UI sample. It does not represent an approved memory or current site behavior.'
	},
	{
		slug: 'naver-draft',
		site: 'Naver Blog',
		domain: 'blog.naver.com',
		title: 'Confirm a draft was saved',
		category: 'Blog writing',
		summary: 'Write a title and body, then verify that the draft is still available.',
		verified: 'Demo data',
		version: 'v2',
		contributors: 5,
		hint: 'Wait for the save indicator, reopen the draft list, and confirm the new item is there before leaving the editor.',
		steps: ['Enter a title and body, then wait for the save indicator.', 'Open the draft list and find the item you just created.', 'Reopen the draft and check that its title and body are intact.'],
		caveat: 'This catalog item is a UI sample. It does not represent an approved memory or current site behavior.'
	},
	{
		slug: 'slides-image',
		site: 'Google Slides',
		domain: 'slides.google.com',
		title: 'Check a slide after adding an image',
		category: 'Presentation',
		summary: 'Place a generated image and check for cropping and save status.',
		verified: 'Demo data',
		version: 'v1',
		contributors: 3,
		hint: 'After placing an image, check that it stays within the slide bounds and remains readable in both presenter and normal views.',
		steps: ['Insert the image on the current slide.', 'Resize from a corner handle while preserving its aspect ratio.', 'Keep it inside the slide bounds and check the autosave status.'],
		caveat: 'This catalog item is a UI sample. It does not represent an approved memory or current site behavior.'
	}
];

export const contributors = [
	{ rank: 1, handle: 'trailmaker', points: 284, contributions: 12, mark: 'TM', color: 'mint' },
	{ rank: 2, handle: 'formfinder', points: 216, contributions: 9, mark: 'FF', color: 'peach' },
	{ rank: 3, handle: 'northstar', points: 184, contributions: 7, mark: 'NS', color: 'blue' },
	{ rank: 4, handle: 'slowcoffee', points: 142, contributions: 6, mark: 'SC', color: 'lavender' },
	{ rank: 5, handle: 'quietorbit', points: 98, contributions: 4, mark: 'QO', color: 'yellow' }
];

export const profileBios: Record<string, { name: string; bio: string; links: { label: string; href: string }[] }> = {
	trailmaker: {
		name: 'Workflow researcher',
		bio: 'I turn repeated detours into small guides that save the next person time.',
		links: [{ label: 'GitHub', href: 'https://github.com' }]
	},
	formfinder: {
		name: 'Forms and editors guide',
		bio: 'I document the small obstacles agents meet in web forms and editors.',
		links: [{ label: 'Website', href: 'https://example.com' }]
	},
	northstar: {
		name: 'Clear next steps',
		bio: 'I like short, repeatable workflows with a visible success check.',
		links: []
	},
	slowcoffee: {
		name: 'Careful progress',
		bio: 'I look for safe next steps in the interface users can actually see.',
		links: []
	},
	quietorbit: {
		name: 'Small discoveries',
		bio: 'A minor detour can become a useful map for someone else.',
		links: []
	}
};

export const analytics = [
	{ label: '10/20', recall: 18, detail: 8, success: 5 },
	{ label: '10/21', recall: 26, detail: 13, success: 9 },
	{ label: '10/22', recall: 21, detail: 11, success: 7 },
	{ label: '10/23', recall: 34, detail: 18, success: 14 },
	{ label: '10/24', recall: 30, detail: 16, success: 12 },
	{ label: '10/25', recall: 42, detail: 25, success: 19 },
	{ label: '10/26', recall: 38, detail: 22, success: 17 }
];
