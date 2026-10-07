// Minimal base58 encoder (Solana addresses), dependency-free.

const ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

export function base58Encode(bytes: Uint8Array): string {
  let zeros = 0;
  while (zeros < bytes.length && bytes[zeros] === 0) zeros += 1;

  const digits: number[] = [0];
  for (let i = zeros; i < bytes.length; i += 1) {
    let carry = bytes[i];
    for (let j = 0; j < digits.length; j += 1) {
      carry += digits[j] << 8;
      digits[j] = carry % 58;
      carry = (carry / 58) | 0;
    }
    while (carry > 0) {
      digits.push(carry % 58);
      carry = (carry / 58) | 0;
    }
  }

  let out = "1".repeat(zeros);
  for (let i = digits.length - 1; i >= 0; i -= 1) out += ALPHABET[digits[i]];
  return out;
}

export function base58Decode(input: string): Uint8Array {
  let zeros = 0;
  while (zeros < input.length && input[zeros] === "1") zeros += 1;

  const bytes: number[] = [];
  for (let i = zeros; i < input.length; i += 1) {
    const value = ALPHABET.indexOf(input[i]);
    if (value === -1) throw new Error(`invalid base58 character: ${input[i]}`);
    let carry = value;
    for (let j = 0; j < bytes.length; j += 1) {
      carry += bytes[j] * 58;
      bytes[j] = carry & 0xff;
      carry >>= 8;
    }
    while (carry > 0) {
      bytes.push(carry & 0xff);
      carry >>= 8;
    }
  }

  const out = new Uint8Array(zeros + bytes.length);
  for (let i = 0; i < bytes.length; i += 1) out[zeros + i] = bytes[bytes.length - 1 - i];
  return out;
}

export function shortAddress(address: string, keep = 4): string {
  if (address.length <= keep * 2 + 3) return address;
  return `${address.slice(0, keep)}…${address.slice(-keep)}`;
}
