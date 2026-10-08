import type { Profile, ProfileSetter } from "../types";
/* Picking a timezone: Auto from the exit IP, or Manual from every zone there is.
 *
 * This was a bare `<input>` beside a single toggle that read "IP" or "Manual"
 * depending on its own state - one control with two meanings, and you had to
 * press it to find out which. It is two buttons now, each saying what it does.
 *
 * The field itself accepted any string and offered no list. The only timezone
 * source the dashboard had was `LOCALES` - ten pairs - so a profile that needed
 * `Asia/Tehran` or `America/Los_Angeles` was typed from memory with nothing to
 * confirm the zone exists; a typo reached the engine and the browser wore a
 * zone that is not a place. `TIMEZONES` asks the platform instead: 418 zones on
 * current Chrome and Node, and it cannot go stale the way a written list does.
 *
 * `<input list=…>` + `<datalist>` rather than a `<select>`, on purpose. The
 * native datalist filters as you type - the search, with no combobox to build,
 * no dependency, and no keyboard or screen-reader behaviour to reimplement -
 * and a stored zone the runtime has since renamed still shows instead of
 * silently snapping to the first option.
 *
 * WHAT IT DELIBERATELY DOES NOT DO: make a non-default zone *coherent*. The
 * authority is `numera/fingerprint.py`, where each locale maps to exactly one
 * zone and anything else is "timezone does not match locale"; `coherence()`
 * mirrors that. So a second US zone is now selectable and still reads as
 * incoherent. Fixing that needs a zone-to-country model in the ENGINE first -
 * offering the list is the half that does not require one.
 *
 * Its own file because `Editor.jsx` is already past the 800-line ceiling and
 * may not grow; this is a whole control with its own rules.
 */

import { Clock, Globe } from "lucide-react";
import { TIMEZONES, validTimezone } from "../deviceData";
import { Field } from "../ui";

//: One list for the document, shared by every instance of the control.
const LIST_ID = "numera-timezones";

function TimezoneField({ p, set, signal = "timezone" }: { p: Pick<Profile, "timezone" | "ipBased" | "proxy">; set: ProfileSetter; signal?: string }) {
  const auto = (p.ipBased || []).includes(signal);
  const hasProxy = !!p.proxy;
  const known = validTimezone(p.timezone);

  const choose = (wantAuto: boolean) => set({
    ipBased: wantAuto
      ? [...new Set([...(p.ipBased || []), signal])]
      : (p.ipBased || []).filter(k => k !== signal),
  });

  return (
    <Field label={<><Clock size={11} /> Timezone</>}>
      <div className="ps-seg">
        <button type="button" className={auto ? "on" : ""} disabled={!hasProxy}
          aria-label="Auto"
          title={hasProxy
            ? "Resolved from the proxy's exit IP each time this profile launches"
            : "Assign a proxy first - there is no exit IP to read"}
          onClick={() => choose(true)}>
          <Globe size={9} /> Auto
        </button>
        <button type="button" className={!auto ? "on" : ""}
          aria-label="Manual"
          title="Choose a zone yourself - type to search all of them"
          onClick={() => choose(false)}>
          Manual
        </button>
      </div>
      {/* Locked only while Auto can actually deliver: a profile with no proxy
          that wants to follow one must stay editable, or the field is dead with
          nothing able to fill it. */}
      {/* No `aria-label` here on purpose: `Field` gives this input the field's
          own label, because it is the only naming-capable control in the field.
          Carrying one as well made the group AND the control both announce
          "Timezone" - the duplication the Field rule exists to prevent, found
          by reading the rendered page rather than the component. The Auto and
          Manual buttons keep their own labels; they say something different. */}
      <input className="ps-in mono" value={p.timezone} list={LIST_ID}
        disabled={auto && hasProxy} placeholder="Region/City" spellCheck={false}
        aria-invalid={!known || undefined}
        title={known ? `One of ${TIMEZONES.length} zones this browser knows`
          : "Not a zone this browser knows - check the spelling"}
        onChange={e => set({ timezone: e.target.value })} />
      <datalist id={LIST_ID}>
        {TIMEZONES.map(z => <option key={z} value={z} />)}
      </datalist>
      {!auto && !known && p.timezone
        ? <span className="ps-hint">Not a zone this browser knows.</span>
        : null}
    </Field>
  );
}

export { TimezoneField, LIST_ID };
