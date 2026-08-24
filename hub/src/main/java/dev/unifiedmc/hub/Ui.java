package dev.unifiedmc.hub;

import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.util.Mth;

/** The look, in one place, so every screen is recognisably the same product. */
public final class Ui {
	public static final int SCRIM_TOP = 0x66000000;
	public static final int SCRIM_BOTTOM = 0xCC000000;

	public static final int CARD_TOP = 0xF21C1C24;
	public static final int CARD_BOTTOM = 0xF2121218;
	public static final int ROW = 0x59202029;
	public static final int ROW_HOVER = 0x8C2C2C3A;
	public static final int EDGE = 0x26FFFFFF;
	public static final int SHADOW = 0x33000000;

	public static final int TRACK = 0xFF26262F;
	public static final int FILL_LEFT = 0xFF3D7FD1;
	public static final int FILL_RIGHT = 0xFF62B4F5;
	public static final int SHEEN = 0x59FFFFFF;

	public static final int TEXT = 0xFFFFFFFF;
	public static final int TEXT_DIM = 0xFF9C9CAC;
	public static final int TEXT_FAINT = 0xFF5E5E6E;
	public static final int ACCENT = 0xFF6FB6F2;
	public static final int GOOD = 0xFF63D18A;
	public static final int WARN = 0xFFE8B84C;
	public static final int BAD = 0xFFE8685C;

	public static final int CORNER = 3;

	private Ui() {
	}

	/** Minecraft cannot draw a rounded rectangle, but three stacked ones read as one. */
	public static void rounded(GuiGraphics graphics, int x1, int y1, int x2, int y2,
			int top, int bottom, int radius) {
		if (x2 - x1 <= radius * 2 || y2 - y1 <= radius * 2) {
			graphics.fillGradient(x1, y1, x2, y2, top, bottom);
			return;
		}
		graphics.fillGradient(x1 + radius, y1, x2 - radius, y2, top, bottom);
		graphics.fillGradient(x1, y1 + radius, x1 + radius, y2 - radius, top, bottom);
		graphics.fillGradient(x2 - radius, y1 + radius, x2, y2 - radius, top, bottom);
	}

	/** A panel with its shadow and top highlight - the shape every screen here is built from. */
	public static void card(GuiGraphics graphics, int x1, int y1, int x2, int y2, float fade) {
		for (int depth = 3; depth >= 1; depth--) {
			rounded(graphics, x1 - depth, y1 - depth + 2, x2 + depth, y2 + depth + 2,
					alpha(SHADOW, fade), alpha(SHADOW, fade), CORNER + depth);
		}
		rounded(graphics, x1, y1, x2, y2, alpha(CARD_TOP, fade), alpha(CARD_BOTTOM, fade), CORNER);
		graphics.fill(x1 + CORNER, y1, x2 - CORNER, y1 + 1, alpha(EDGE, fade));
	}

	public static void scrim(GuiGraphics graphics, int width, int height, float fade) {
		graphics.fillGradient(0, 0, width, height, alpha(SCRIM_TOP, fade), alpha(SCRIM_BOTTOM, fade));
	}

	public static void scaled(GuiGraphics graphics, Font font, int x, int y, float scale,
			String text, int colour, boolean centred) {
		graphics.pose().pushMatrix();
		graphics.pose().translate(x, y);
		graphics.pose().scale(scale, scale);
		if (centred) {
			graphics.drawCenteredString(font, text, 0, 0, colour);
		} else {
			graphics.drawString(font, text, 0, 0, colour, false);
		}
		graphics.pose().popMatrix();
	}

	public static int alpha(int colour, float factor) {
		int a = (int) (((colour >>> 24) & 0xFF) * Mth.clamp(factor, 0f, 1f));
		return (a << 24) | (colour & 0x00FFFFFF);
	}

	public static String trim(Font font, String text, int room) {
		return room <= 8 || font.width(text) <= room
				? text
				: font.plainSubstrByWidth(text, room - 8) + "...";
	}

	/** Smoothstep: things that appear should arrive, not pop. */
	public static float ease(float t) {
		t = Mth.clamp(t, 0f, 1f);
		return t * t * (3f - 2f * t);
	}
}
