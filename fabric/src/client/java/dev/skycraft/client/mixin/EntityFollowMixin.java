package dev.skycraft.client.mixin;

import dev.skycraft.client.FollowStep;
import dev.skycraft.client.SkyClient;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.ModifyVariable;

/**
 * EldenCraft follow mode. Elden Ring moves the player itself (Minecraft's physics numbers against Elden
 * Ring's real collision), so the local player's movement each tick is exactly "to where Elden Ring has
 * it". Vanilla move() still runs (walk animation, fall damage, onGround, stats, step sounds): the
 * movement asked for is pointed at the target, slightly into the ground when the host says it stands
 * on ground (so vanilla sees a landing), and EntityCollideMixin returns the exact step.
 */
@Mixin(Entity.class)
public abstract class EntityFollowMixin {
	@ModifyVariable(method = "move", at = @At("HEAD"), argsOnly = true)
	private Vec3 eldencraft$followHost(Vec3 movement) {
		if (!((Object) this instanceof LocalPlayer player) || !SkyClient.following()) {
			return movement;
		}
		var sky = SkyClient.sky();
		Vec3 step = new Vec3(sky.x - player.getX(), sky.y - player.getY(), sky.z - player.getZ());
		FollowStep.pending = step;
		if (sky.followOnGround()) {
			return new Vec3(step.x, Math.min(step.y, 0.0) - 0.08, step.z);
		}
		return step;
	}
}
