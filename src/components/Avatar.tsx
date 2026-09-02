import { avatarColour } from "../format";

interface AvatarProps {
  /** Two letters, already uppercased. */
  initials: string;
  /** Anything stable about the person — the same seed always gives the same colour. */
  seed: string;
  size?: number;
}

export function Avatar({ initials, seed, size = 34 }: AvatarProps) {
  return (
    <span
      className="avatar"
      aria-hidden="true"
      style={{
        width: size,
        height: size,
        background: avatarColour(seed),
        fontSize: Math.round(size * 0.35),
      }}
    >
      {initials}
    </span>
  );
}
