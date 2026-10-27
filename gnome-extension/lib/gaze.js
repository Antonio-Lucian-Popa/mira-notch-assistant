// A bounded elliptical gaze keeps the direction even far from the notch.
export function gazeOffset(dx, dy) {
    const distance = Math.hypot(dx, dy);
    const strength = Math.min(1, distance / 600);
    if (!distance)
        return {x: 0, y: 0};
    return {x: dx / distance * strength * 8, y: dy / distance * strength * 5};
}
