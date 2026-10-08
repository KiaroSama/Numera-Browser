import { Globe } from "lucide-react";
import type { Profile, ProfileSetter } from "../types";

export function IpSwitch({ p, set, signal }: { p: Profile; set: ProfileSetter; signal: string }) {
  const on = (p.ipBased || []).includes(signal);
  const has = !!p.proxy;
  return (
    <button type="button" className={"ps-ipsw" + (on ? " on" : "")}
      title={!has ? "Assign a proxy first - there is no exit IP to read"
        : on ? "Resolved from the proxy's exit IP each time this profile launches"
             : "Uses the value set here"}
      disabled={!has}
      onClick={() => set({ ipBased: on ? (p.ipBased || []).filter(k => k !== signal)
                                       : [...(p.ipBased || []), signal] })}>
      <Globe size={9} /> {on ? "IP" : "Manual"}
    </button>
  );
}
