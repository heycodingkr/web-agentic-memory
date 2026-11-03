export function posterTimeForRange(preferredTime, from, to) {
  if (!Number.isFinite(from) || !Number.isFinite(to) || from < 0 || to <= from) {
    throw new RangeError('render range must be finite, non-negative, and non-empty');
  }
  if (Number.isFinite(preferredTime) && preferredTime >= from && preferredTime < to) {
    return preferredTime;
  }
  return from + (to - from) / 2;
}

export function overlayOutputLimitArgs(frameCount) {
  if (!Number.isSafeInteger(frameCount) || frameCount < 1) {
    throw new RangeError('overlay output frame count must be a positive safe integer');
  }
  return ['-frames:v', String(frameCount)];
}
