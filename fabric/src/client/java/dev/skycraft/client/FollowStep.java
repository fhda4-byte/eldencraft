package dev.skycraft.client;

import net.minecraft.world.phys.Vec3;

/** The exact step EntityFollowMixin asked for this move (client thread only). */
public final class FollowStep {
	public static Vec3 pending;

	private FollowStep() {
	}
}
