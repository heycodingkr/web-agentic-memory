const DEVICE_LINK_CODE = /^([A-HJ-NP-Z2-9]{4})-?([A-HJ-NP-Z2-9]{4})$/i;

export function normalizeDeviceLinkCode(value: string | null | undefined): string {
  const match = DEVICE_LINK_CODE.exec(value?.trim() ?? '');
  return match ? `${match[1].toUpperCase()}-${match[2].toUpperCase()}` : '';
}
