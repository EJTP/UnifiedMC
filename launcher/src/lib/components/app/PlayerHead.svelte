<script lang="ts">
	/**
	 * The player's head, as a head rather than a stamp.
	 *
	 * Six divs in a `preserve-3d` box, each showing one 8x8 square of the skin through
	 * `background-position` - so the whole thing is one image and no slicing happens anywhere.
	 * A second, very slightly larger cube carries the overlay layer, which is what gives a
	 * skin its hat, hair or helmet; without it half of all skins render bald.
	 *
	 * CSS rather than a renderer: this is six textured quads. A WebGL dependency to draw a
	 * cube would be more code to ship, more to load, and no more convincing.
	 */
	let {
		/** The whole 64x64 skin as a data url. Null while it is still being fetched. */
		skin = null,
		/**
		 * The flat face, for when the whole skin could not be fetched. A cube needs six
		 * squares; one 8x8 face is still a face, and showing it beats a grey box.
		 */
		fallback = null,
		size = 48,
		/** Whether the head follows the pointer. Off for a decorative one nobody will touch. */
		interactive = true
	}: {
		skin?: string | null;
		fallback?: string | null;
		size?: number;
		interactive?: boolean;
	} = $props();

	/**
	 * Where each face lives in a 64x64 skin, as (x, y) of its top-left pixel.
	 *
	 * The overlay layer is the same six squares shifted 32 to the right, which is the whole
	 * reason this is a table and not twelve hand-written positions.
	 */
	const FACES = [
		{ name: "front", x: 8, y: 8, transform: "translateZ(var(--half))" },
		{ name: "back", x: 24, y: 8, transform: "rotateY(180deg) translateZ(var(--half))" },
		{ name: "right", x: 0, y: 8, transform: "rotateY(-90deg) translateZ(var(--half))" },
		{ name: "left", x: 16, y: 8, transform: "rotateY(90deg) translateZ(var(--half))" },
		{ name: "top", x: 8, y: 0, transform: "rotateX(90deg) translateZ(var(--half))" },
		{ name: "bottom", x: 16, y: 0, transform: "rotateX(-90deg) translateZ(var(--half))" }
	];

	/** The overlay's own squares sit exactly 32 pixels right of the base ones. */
	const OVERLAY_OFFSET = 32;

	/**
	 * How much larger the overlay cube is. Minecraft draws the hat layer half a pixel proud on
	 * every side, which on an eight-wide head is exactly 9/8 - so this is the game's number
	 * rather than one picked to look right.
	 */
	const OVERLAY_SCALE = 1.125;

	/** A resting three-quarter view: face on, turned just enough to show it has sides. */
	let yaw = $state(-22);
	let pitch = $state(10);

	function follow(event: PointerEvent) {
		if (!interactive) return;
		const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
		// Normalised to -0.5..0.5 across the element, then to a range that stays readable:
		// past about 35 degrees the face turns away, which is not what a portrait is for.
		const x = (event.clientX - box.left) / box.width - 0.5;
		const y = (event.clientY - box.top) / box.height - 0.5;
		yaw = x * 70;
		pitch = -y * 50;
	}

	function rest() {
		yaw = -22;
		pitch = 10;
	}

	/**
	 * One skin pixel, in whole screen pixels.
	 *
	 * Rounded, and the cube then sized from it rather than the other way round: at a requested
	 * 30px a skin pixel would be 3.75 screen pixels, and nearest-neighbour on a fractional grid
	 * gives some pixels three columns and some four. That unevenness is what reads as blur.
	 * Snapping costs a pixel or two of size and makes every square identical.
	 */
	const unit = $derived(Math.max(1, Math.round(size / 8)));

	/** What the cube actually ends up being: eight whole skin pixels across. */
	const px = $derived(unit * 8);
</script>

<!--
	The listener sits on the outer box rather than the cube: the cube is rotated, so its own
	bounding box moves as it turns and the pointer maths would chase itself.
-->
<div
	class="grid shrink-0 place-items-center [image-rendering:pixelated]"
	style:width="{px}px"
	style:height="{px}px"
	style:perspective="{px * 10}px"
	onpointermove={follow}
	onpointerleave={rest}
	aria-hidden="true"
>
	{#if skin}
		<div
			class="relative transition-transform duration-300 ease-out [transform-style:preserve-3d]"
			style:width="{px}px"
			style:height="{px}px"
			style:transform="rotateX({pitch}deg) rotateY({yaw}deg)"
			style:--half="{px / 2}px"
		>
			{#each [0, OVERLAY_OFFSET] as layer (layer)}
				{@const isOverlay = layer > 0}
				<!--
					The overlay is grown a hair rather than placed exactly on the base: two
					coplanar faces z-fight, and the flicker is worse than the gap is visible.
				-->
				<div
					class="absolute inset-0 [transform-style:preserve-3d]"
					style:transform={isOverlay ? `scale(${OVERLAY_SCALE})` : undefined}
				>
					{#each FACES as face (face.name)}
						<div
							class="absolute inset-0 [backface-visibility:hidden] [image-rendering:pixelated]"
							style:background-image="url({skin})"
							style:background-size="{unit * 64}px {unit * 64}px"
							style:background-position="-{(face.x + layer) * unit}px -{face.y * unit}px"
							style:transform={face.transform}
						></div>
					{/each}
				</div>
			{/each}
		</div>
	{:else if fallback}
		<img src={fallback} alt="" class="size-full rounded-sm [image-rendering:pixelated]" />
	{:else}
		<!-- Same footprint while it loads, so the row it sits in does not jump when it arrives. -->
		<div class="size-full rounded-sm bg-muted"></div>
	{/if}
</div>
