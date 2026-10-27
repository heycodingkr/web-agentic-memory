<script lang="ts">
	import type { BioBlock, BioInline } from '../api';

	let { value, onchange }: { value: BioBlock[]; onchange: (blocks: BioBlock[]) => void } = $props();
	let text = $state('');
	let format = $state<'paragraph' | 'heading' | 'bullet_list' | 'ordered_list'>('paragraph');
	let emphasis = $state<'normal' | 'strong' | 'emphasis'>('normal');
	let linkUrl = $state('');

	function addBlock() {
		const clean = text.trim();
		if (!clean) return;
		let content: BioInline[] = [{ type: 'text', text: clean }];
		if (emphasis === 'strong') content = [{ type: 'strong', text: clean }];
		if (emphasis === 'emphasis') content = [{ type: 'emphasis', text: clean }];
		if (linkUrl.trim()) content = [{ type: 'link', text: clean, url: linkUrl.trim() }];
		let block: BioBlock;
		if (format === 'heading') block = { type: 'heading', level: 2, content };
		else if (format === 'bullet_list') block = { type: 'bullet_list', items: [content] };
		else if (format === 'ordered_list') block = { type: 'ordered_list', items: [content] };
		else block = { type: 'paragraph', content };
		onchange([...value, block]);
		text = '';
		linkUrl = '';
	}

	function removeBlock(index: number) {
		onchange(value.filter((_, blockIndex) => blockIndex !== index));
	}
</script>

<section class="rich-editor" aria-labelledby="bio-editor-heading">
	<div class="editor-title"><h2 id="bio-editor-heading">Edit your bio</h2><p>Save text, formatting, and HTTPS links as structured blocks.</p></div>
	<div class="editor-row">
		<label>Block type
			<select bind:value={format}>
				<option value="paragraph">Paragraph</option><option value="heading">Heading</option>
				<option value="bullet_list">Bulleted list</option><option value="ordered_list">Numbered list</option>
			</select>
		</label>
		<label>Text style
			<select bind:value={emphasis}>
				<option value="normal">Normal</option><option value="strong">Bold</option><option value="emphasis">Italic</option>
			</select>
		</label>
	</div>
	<label>Text
		<textarea bind:value={text} rows="3" maxlength="1200" placeholder="Tell people about yourself and your work."></textarea>
	</label>
	<label>Link for this text (optional)
		<input bind:value={linkUrl} type="url" inputmode="url" placeholder="https://example.com" />
	</label>
	<button class="button button-light button-small" type="button" onclick={addBlock}>Add block</button>

	{#if value.length}
		<ol class="blocks" aria-label="Bio blocks">
			{#each value as block, index}
				<li><span>{index + 1}. {block.type}</span><button type="button" aria-label={`Remove block ${index + 1}`} onclick={() => removeBlock(index)}>Remove</button></li>
			{/each}
		</ol>
	{/if}
</section>

<style>
	.rich-editor { display: grid; gap: 13px; }
	.editor-title h2 { margin: 0 0 5px; font: 600 16px Manrope, sans-serif; }
	.editor-title p { color: #78857d; font-size: 11px; line-height: 1.6; margin: 0; }
	.rich-editor label { display: grid; gap: 6px; color: #637268; font-size: 11px; font-weight: 600; }
	.rich-editor input, .rich-editor textarea, .rich-editor select { width: 100%; min-height: 42px; padding: 10px 11px; border: 1px solid #dce2d9; border-radius: 6px; color: #18312a; background: #fffef9; font: inherit; }
	.rich-editor textarea { resize: vertical; line-height: 1.6; }
	.editor-row { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
	.blocks { display: grid; gap: 6px; list-style: none; padding: 0; margin: 0; }
	.blocks li { display: flex; justify-content: space-between; border-top: 1px solid #e7ebe5; padding: 8px 0; color: #718077; font-size: 11px; }
	.blocks button { border: 0; background: none; color: #a0523e; cursor: pointer; }
</style>
