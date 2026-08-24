package dev.unifiedmc.hub;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.network.chat.Component;
import net.minecraft.util.Mth;

import static dev.unifiedmc.hub.Ui.*;

/**
 * What the player looks at while the shell provisions the next instance behind this window.
 *
 * The work happens in another process, so this screen has nothing of its own to report - it reads
 * what the shell writes and draws it. Without that the player stares at a spinner through a four
 * hundred megabyte download and assumes the thing has hung.
 */
public class HandoffScreen extends Screen {
	private static final Path PROGRESS =
			Path.of(System.getProperty("user.home"), ".unifiedmc", "progress.json");

	/** Re-reading a file every frame is wasteful; nothing here changes faster than the eye cares. */
	private static final long POLL_MS = 150;

	private static final int CARD_WIDTH = 320;
	private static final int CARD_HEIGHT = 96;
	private static final int BAR_HEIGHT = 8;
	private static final int PAD = 18;




	private static final float FADE_MS = 220f;
	private static final float TITLE_SCALE = 1.35f;

	private final String target;

	private long lastPoll;
	private long startedAt = -1;
	private long lastFrame;

	/** Eased towards the reported fraction, so a burst of finished downloads glides instead of jumping. */
	private float shown;

	private String phase = "Verbindung wird vorbereitet";
	private String detail = "";
	private int done;
	private int total;

	public HandoffScreen(String target) {
		super(Component.literal("UnifiedMC"));
		this.target = target;
	}

	@Override
	public boolean shouldCloseOnEsc() {
		return false;   // the shell owns this window's lifetime, not the player
	}

	private void poll(long now) {
		if (now - lastPoll < POLL_MS) {
			return;
		}
		lastPoll = now;
		try {
			if (!Files.exists(PROGRESS)) {
				return;
			}
			JsonObject reported = JsonParser.parseString(Files.readString(PROGRESS)).getAsJsonObject();
			phase = reported.get("phase").getAsString();
			detail = reported.has("detail") ? reported.get("detail").getAsString() : "";
			done = reported.has("done") ? reported.get("done").getAsInt() : 0;
			total = reported.has("total") ? reported.get("total").getAsInt() : 0;
		} catch (IOException | RuntimeException e) {
			// a torn or missing read just leaves the last known state on screen
		}
	}

	@Override
	public void render(GuiGraphics graphics, int mouseX, int mouseY, float partialTick) {
		long now = System.currentTimeMillis();
		if (startedAt < 0) {
			startedAt = now;
			lastFrame = now;
		}
		poll(now);

		float elapsed = now - startedAt;
		float fade = Mth.clamp(elapsed / FADE_MS, 0f, 1f);
		fade = fade * fade * (3f - 2f * fade);   // smoothstep: no hard pop when the screen appears

		super.render(graphics, mouseX, mouseY, partialTick);
		graphics.fillGradient(0, 0, this.width, this.height, Ui.alpha(SCRIM_TOP, fade), Ui.alpha(SCRIM_BOTTOM, fade));

		int left = (this.width - CARD_WIDTH) / 2;
		int top = (this.height - CARD_HEIGHT) / 2;
		int middle = this.width / 2;

		for (int depth = 3; depth >= 1; depth--) {
			Ui.rounded(graphics, left - depth, top - depth + 2, left + CARD_WIDTH + depth,
					top + CARD_HEIGHT + depth + 2, Ui.alpha(SHADOW, fade), Ui.alpha(SHADOW, fade), CORNER + depth);
		}
		Ui.rounded(graphics, left, top, left + CARD_WIDTH, top + CARD_HEIGHT,
				Ui.alpha(CARD_TOP, fade), Ui.alpha(CARD_BOTTOM, fade), CORNER);
		graphics.fill(left + CORNER, top, left + CARD_WIDTH - CORNER, top + 1, Ui.alpha(EDGE, fade));

		graphics.drawCenteredString(this.font, "U N I F I E D M C", middle, top + 12, Ui.alpha(TEXT_FAINT, fade));
		Ui.scaled(graphics, this.font, middle, top + 26, TITLE_SCALE, target, Ui.alpha(TEXT, fade), true);
		graphics.drawCenteredString(this.font, phase + dots(now), middle, top + 50, Ui.alpha(ACCENT, fade));

		int barLeft = left + PAD;
		int barRight = left + CARD_WIDTH - PAD;
		int barTop = top + 64;
		bar(graphics, barLeft, barTop, barRight - barLeft, now, fade);

		int footer = barTop + BAR_HEIGHT + 6;
		if (total > 0) {
			String count = done + " / " + total;
			graphics.drawString(this.font, count, barRight - this.font.width(count), footer,
					Ui.alpha(TEXT_DIM, fade), false);
		}
		if (!detail.isEmpty()) {
			int room = barRight - barLeft - (total > 0 ? this.font.width(done + " / " + total) + 8 : 0);
			graphics.drawString(this.font, Ui.trim(this.font, detail, room), barLeft, footer, Ui.alpha(TEXT_FAINT, fade), false);
		}
		lastFrame = now;
	}

	/** A known count fills proportionally; an unknown one sweeps, so no step ever looks frozen. */
	private void bar(GuiGraphics graphics, int x, int y, int width, long now, float fade) {
		Ui.rounded(graphics, x, y, x + width, y + BAR_HEIGHT, Ui.alpha(TRACK, fade), Ui.alpha(TRACK, fade), 2);

		int filled;
		if (total > 0) {
			float wanted = Mth.clamp((float) done / total, 0f, 1f);
			// ease towards the target at a rate independent of frame time
			shown += (wanted - shown) * Mth.clamp((now - lastFrame) / 120f, 0f, 1f);
			filled = Math.round(width * shown);
			if (filled < 2) {
				return;
			}
			gradientRounded(graphics, x, y, x + filled, fade);
			sheen(graphics, x, y, filled, now, fade);
			return;
		}

		int sweep = Math.max(28, width / 4);
		int travel = width + sweep;
		int at = (int) ((now - startedAt) / 5 % travel) - sweep;
		int from = Math.max(x, x + at);
		int to = Math.min(x + width, x + at + sweep);
		if (to - from > 1) {
			gradientRounded(graphics, from, y, to, fade);
		}
	}

	private void gradientRounded(GuiGraphics graphics, int x, int y, int right, float fade) {
		graphics.fillGradient(x, y + 1, right, y + BAR_HEIGHT - 1, Ui.alpha(FILL_LEFT, fade), Ui.alpha(FILL_RIGHT, fade));
		graphics.fill(x + 1, y, right - 1, y + 1, Ui.alpha(FILL_LEFT, fade));
		graphics.fill(x + 1, y + BAR_HEIGHT - 1, right - 1, y + BAR_HEIGHT, Ui.alpha(FILL_RIGHT, fade));
	}

	/** A highlight travelling along the filled part - the difference between "working" and "stuck". */
	private void sheen(GuiGraphics graphics, int x, int y, int filled, long now, float fade) {
		int band = 24;
		int at = (int) ((now - startedAt) / 6 % (filled + band)) - band;
		int from = Math.max(x, x + at);
		int to = Math.min(x + filled, x + at + band);
		if (to - from > 1) {
			graphics.fillGradient(from, y + 1, to, y + BAR_HEIGHT - 1, Ui.alpha(SHEEN, fade * 0.5f), Ui.alpha(SHEEN, 0f));
		}
	}

	private static String dots(long now) {
		return ".".repeat((int) (now / 400 % 4));
	}

}
