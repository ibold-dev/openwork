/** Presentation helpers. Money is always bigint base units, never a float. */

export function shortenAddress(address: string, visible = 4): string {
  if (address.length <= visible * 2 + 3) return address;
  return `${address.slice(0, visible)}…${address.slice(-visible)}`;
}

/**
 * Formats base units as a decimal string with grouping. No rounding of value:
 * the fractional part is truncated to `maxFractionDigits` and trailing zeros are
 * removed.
 */
export function formatAmount(
  baseUnits: bigint,
  decimals: number,
  options: { maxFractionDigits?: number; grouping?: boolean } = {},
): string {
  const { maxFractionDigits = Math.min(decimals, 6), grouping = true } = options;
  const negative = baseUnits < 0n;
  const absolute = negative ? -baseUnits : baseUnits;
  const scale = 10n ** BigInt(decimals);
  const whole = absolute / scale;
  const fraction = absolute % scale;

  let fractionText = fraction.toString().padStart(decimals, '0');
  if (maxFractionDigits < decimals) {
    fractionText = fractionText.slice(0, maxFractionDigits);
  }
  fractionText = fractionText.replace(/0+$/, '');

  const wholeText = grouping
    ? whole.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',')
    : whole.toString();
  const sign = negative ? '−' : '';
  return fractionText.length > 0
    ? `${sign}${wholeText}.${fractionText}`
    : `${sign}${wholeText}`;
}

/** Parses a human decimal string into base units without floating point. */
export function parseAmount(value: string, decimals: number): bigint | null {
  const trimmed = value.trim().replace(/,/g, '');
  if (!/^\d+(\.\d*)?$/.test(trimmed)) return null;
  const [whole = '0', fraction = ''] = trimmed.split('.');
  if (fraction.length > decimals) return null;
  const padded = fraction.padEnd(decimals, '0');
  try {
    return BigInt(whole) * 10n ** BigInt(decimals) + BigInt(padded || '0');
  } catch {
    return null;
  }
}

export function formatTimestamp(unixSeconds: bigint | number): string {
  const value = typeof unixSeconds === 'bigint' ? Number(unixSeconds) : unixSeconds;
  if (!value) return '—';
  return new Date(value * 1000).toLocaleString(undefined, {
    year: 'numeric',
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function relativeTime(unixSeconds: bigint | number): string {
  const value = typeof unixSeconds === 'bigint' ? Number(unixSeconds) : unixSeconds;
  if (!value) return '—';
  const delta = Date.now() / 1000 - value;
  const minutes = Math.round(Math.abs(delta) / 60);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.round(hours / 24);
  return `${days}d ago`;
}

export function titleCase(value: string): string {
  return value.replace(/(^|\s)\w/g, (match) => match.toUpperCase());
}

export function bytesToHexString(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
}
