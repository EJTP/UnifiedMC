<script lang="ts">
	/**
	 * The player's head, as a head rather than a stamp.
	 *
	 * Drawn on a canvas rather than built out of CSS 3D boxes. The CSS version was correct and
	 * worked in Chrome, in the dev server, and against the system WebKitGTK - but a Tauri
	 * AppImage ships its own WebKit, and that one flattens `transform-style: preserve-3d`.
	 * Flattened, a face turned ninety degrees collapses to a horizontal sliver, which is all
	 * the head ever was on a real install. Nothing here needs the engine to composite in 3D.
	 *
	 * The projection is orthographic, which makes every face an exact parallelogram - so the
	 * canvas transform mapping an 8x8 square of the skin onto it is exact rather than an
	 * approximation, and `imageSmoothingEnabled = false` keeps the pixels square.
	 */
	let {
		/** The whole 64x64 skin as a data url. Null while it is still being fetched. */
		skin = null,
		/** The flat face, for when the whole skin could not be fetched at all. */
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

	type Vec = [number, number, number];

	/**
	 * The six faces of a head, as a corner and two edges of a unit cube, plus where that face
	 * lives in the skin. x runs right, y runs down, z runs towards the viewer.
	 *
	 * A skin is painted from the player's own point of view, so the right cheek is the one a
	 * viewer sees on the left.
	 */
	const FACES: { tex: [number, number]; origin: Vec; u: Vec; v: Vec; normal: Vec }[] = [
		{ tex: [8, 8], origin: [-0.5, -0.5, 0.5], u: [1, 0, 0], v: [0, 1, 0], normal: [0, 0, 1] },
		{ tex: [24, 8], origin: [0.5, -0.5, -0.5], u: [-1, 0, 0], v: [0, 1, 0], normal: [0, 0, -1] },
		{ tex: [0, 8], origin: [-0.5, -0.5, -0.5], u: [0, 0, 1], v: [0, 1, 0], normal: [-1, 0, 0] },
		{ tex: [16, 8], origin: [0.5, -0.5, 0.5], u: [0, 0, -1], v: [0, 1, 0], normal: [1, 0, 0] },
		{ tex: [8, 0], origin: [-0.5, -0.5, -0.5], u: [1, 0, 0], v: [0, 0, 1], normal: [0, -1, 0] },
		{ tex: [16, 0], origin: [-0.5, 0.5, 0.5], u: [1, 0, 0], v: [0, 0, -1], normal: [0, 1, 0] }
	];

	/** The overlay's squares sit 32 to the right of the base ones. */
	const OVERLAY_OFFSET = 32;
	/** Minecraft draws the hat half a pixel proud on every side: on an eight-wide head, 9/8. */
	const OVERLAY_SCALE = 1.125;
	/** One square of a skin. */
	const FACE = 8;

	/**
	 * A resting three-quarter view: face on, turned just enough to show it has sides, and
	 * tipped so a little of the top shows.
	 *
	 * Pitch is negative for that. y runs down, so a positive pitch turns the head's underside
	 * towards the viewer - which is the chin-up look that made the first version odd.
	 */
	const REST_YAW = -0.38;
	const REST_PITCH = -0.2;
	/** As far down as it will look, and the nearest it comes to level. */
	const LOOK_DOWN = -0.55;
	const LEVEL = -0.08;

	/** Where the head is turning to, and where it has got to. */
	let wantYaw = $state(REST_YAW);
	let wantPitch = $state(REST_PITCH);
	let yaw = $state(REST_YAW);
	let pitch = $state(REST_PITCH);

	let canvas = $state<HTMLCanvasElement | null>(null);
	let texture = $state<HTMLImageElement | null>(null);

	/** Whole screen pixels per skin pixel, so nearest-neighbour lands on a clean grid. */
	const unit = $derived(Math.max(1, Math.round(size / FACE)));
	const px = $derived(unit * FACE);
	/** Room around the cube: turned, its corners reach past the face's own width. */
	const canvasPx = $derived(Math.round(px * 1.5));

	function turn(p: Vec): Vec {
		const [x, y, z] = p;
		const cy = Math.cos(yaw);
		const sy = Math.sin(yaw);
		const cx = Math.cos(pitch);
		const sx = Math.sin(pitch);
		const x1 = x * cy + z * sy;
		const z1 = -x * sy + z * cy;
		return [x1, y * cx - z1 * sx, y * sx + z1 * cx];
	}

	$effect(() => {
		if (!skin) {
			texture = null;
			return;
		}
		const image = new Image();
		image.onload = () => (texture = image);
		image.src = skin;
	});

	// Reads yaw, pitch, the texture and the sizes, so it redraws whenever any of them moves.
	$effect(() => {
		const target = canvas;
		const image = texture;
		if (!target || !image) return;

		const dpr = Math.max(1, Math.min(3, window.devicePixelRatio || 1));
		target.width = Math.round(canvasPx * dpr);
		target.height = Math.round(canvasPx * dpr);

		const paint = target.getContext("2d");
		if (!paint) return;
		paint.setTransform(1, 0, 0, 1, 0, 0);
		paint.clearRect(0, 0, target.width, target.height);
		paint.imageSmoothingEnabled = false;

		const middle = (canvasPx * dpr) / 2;
		const scale = px * dpr;

		// Both layers at once, sorted far to near: the overlay is larger, so some of its faces
		// belong behind the head and some in front, and drawing it as one block would put its
		// backside over the face.
		const drawn: { depth: number; go: () => void }[] = [];

		for (const layer of [0, OVERLAY_OFFSET]) {
			const grow = layer === 0 ? 1 : OVERLAY_SCALE;
			for (const face of FACES) {
				// A face pointing away is skipped, as Minecraft skips it: through a transparent
				// hat you would otherwise see the inside of its own far side.
				if (turn(face.normal)[2] <= 0.0001) continue;

				const at = (p: Vec): [number, number] => {
					const [x, y] = turn([p[0] * grow, p[1] * grow, p[2] * grow]);
					return [middle + x * scale, middle + y * scale];
				};
				const o = at(face.origin);
				const eu = at([
					face.origin[0] + face.u[0],
					face.origin[1] + face.u[1],
					face.origin[2] + face.u[2]
				]);
				const ev = at([
					face.origin[0] + face.v[0],
					face.origin[1] + face.v[1],
					face.origin[2] + face.v[2]
				]);

				const centre: Vec = [
					face.origin[0] + (face.u[0] + face.v[0]) / 2,
					face.origin[1] + (face.u[1] + face.v[1]) / 2,
					face.origin[2] + (face.u[2] + face.v[2]) / 2
				];
				const depth = turn([centre[0] * grow, centre[1] * grow, centre[2] * grow])[2];

				const [sx, sy] = face.tex;
				drawn.push({
					depth,
					go: () => {
						paint.save();
						// Maps the 8x8 source square exactly onto this face's parallelogram.
						paint.setTransform(
							(eu[0] - o[0]) / FACE,
							(eu[1] - o[1]) / FACE,
							(ev[0] - o[0]) / FACE,
							(ev[1] - o[1]) / FACE,
							o[0],
							o[1]
						);
						// A hair of overlap, or the seam between two faces lets the background
						// show through at some angles.
						paint.drawImage(image, sx + layer, sy, FACE, FACE, 0, 0, FACE + 0.02, FACE + 0.02);
						paint.restore();
					}
				});
			}
		}

		drawn.sort((a, b) => a.depth - b.depth);
		for (const face of drawn) face.go();
	});

	let box = $state<HTMLDivElement | null>(null);
	let lastMoved = 0;

	/**
	 * Follow the pointer anywhere in the window, not only across the head itself.
	 *
	 * The angle comes from the direction to the cursor, softened by distance: something a few
	 * hundred pixels away turns the head fully, and past that it stops rather than straining.
	 */
	function towards(event: PointerEvent) {
		if (!interactive || !box) return;
		const r = box.getBoundingClientRect();
		const dx = event.clientX - (r.left + r.width / 2);
		const dy = event.clientY - (r.top + r.height / 2);
		const reach = 420;
		const clamp = (n: number) => Math.max(-1, Math.min(1, n / reach));
		// Past about forty degrees the face turns away, which is not what a portrait is for.
		wantYaw = clamp(dx) * 0.7;
		// A cursor below means the head looks down, which shows the top of it - hence the sign.
		//
		// Never past level, though. This head lives at the bottom of the sidebar, so the
		// pointer is nearly always above it, and letting it tip that far back would park it
		// on its own chin. The underside of a skin is the one face nobody textures - it is
		// usually a dark band, because it was never meant to be looked at.
		wantPitch = Math.max(LOOK_DOWN, Math.min(LEVEL, -clamp(dy) * 0.45));
		lastMoved = performance.now();
	}

	$effect(() => {
		if (!interactive) return;
		window.addEventListener("pointermove", towards);
		return () => window.removeEventListener("pointermove", towards);
	});

	/**
	 * Ease towards wherever it is meant to be looking, and breathe a little when nothing has
	 * moved for a while - a portrait that is perfectly still reads as a broken image.
	 *
	 * One frame loop for both, because they are the same number: the idle sway is an offset
	 * added to the target rather than a second animation fighting the first.
	 */
	$effect(() => {
		const calm = window.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches ?? false;
		if (calm) {
			yaw = wantYaw;
			pitch = Math.min(wantPitch, LEVEL / 2);
			return;
		}
		let frame = requestAnimationFrame(function tick(now: number) {
			const idle = now - lastMoved > 2500;
			// A slow figure of eight, small enough to notice only if you are looking at it.
			const sway = idle ? Math.sin(now / 1800) * 0.09 : 0;
			const bob = idle ? Math.sin(now / 1300) * 0.045 : 0;
			yaw += (wantYaw + sway - yaw) * 0.08;
			pitch += (wantPitch + bob - pitch) * 0.08;
			// The sway must not push it past level either, or the chin shows on the upswing.
			pitch = Math.min(pitch, LEVEL / 2);
			frame = requestAnimationFrame(tick);
		});
		return () => cancelAnimationFrame(frame);
	});
</script>

<div
	class="grid shrink-0 place-items-center"
	style:width="{canvasPx}px"
	style:height="{canvasPx}px"
	bind:this={box}
	aria-hidden="true"
>
	{#if skin}
		<canvas
			bind:this={canvas}
			style:width="{canvasPx}px"
			style:height="{canvasPx}px"
			class="[image-rendering:pixelated]"
		></canvas>
	{:else if fallback}
		<img
			src={fallback}
			alt=""
			class="rounded-sm [image-rendering:pixelated]"
			style:width="{px}px"
			style:height="{px}px"
		/>
	{:else}
		<!-- Same footprint while it loads, so the row it sits in does not jump when it arrives. -->
		<div class="rounded-sm bg-muted" style:width="{px}px" style:height="{px}px"></div>
	{/if}
</div>
