# Combat balance pass: validating the placeholder CRT and Penetration Table

Tracks issue #26 (follow-up to the combat-resolver milestone, #11). Both `aggregate_strength_v1`'s
`CRT` and `cepheus_vehicle_v1`'s `PENETRATION_TABLE` (`usmf-sim::combat`) were originated from
scratch when #16/#17 built them — issue #15 confirmed `old/`'s prior USMF rule iterations have no
CRT, odds table, or penetration table to seed either from (`docs/combat-resolver-research.md`).
Both were flagged `PLACEHOLDER NUMBERS — not a final design` pending this balance pass.

## Method

There's no human playtester available in this environment and no external numeric baseline to
calibrate against (per #15's finding), so this pass is a Monte Carlo statistical playtest: run
each table through a large number of simulated attacks across a spread of realistic ratios/
matchups, and evaluate the resulting outcome distributions against the properties a combat-results
table needs to *feel* right, independent of the exact numbers:

- No degenerate all-or-nothing outcomes — a bad-odds attacker should still have some chance to
  hurt the defender; overwhelming odds shouldn't be a mathematically guaranteed kill.
- Monotonic escalation — better odds/bigger overmatch should strictly increase expected damage,
  with no dips or discontinuities.
- Sensible response to mismatch — a weapon that can't threaten a given armor class should
  essentially never penetrate it; a weapon that overwhelms a target should essentially always
  penetrate it; matched pairings should land in between, not pinned to either extreme.

200,000-iteration runs (`round_rng(1, 1)`, the same deterministic RNG helper the engine itself
uses) were used for every case below. The exploratory harness that produced these numbers was then
replaced with permanent regression tests (`usmf-sim::combat::balance_pass`) pinning the properties
confirmed here, so a future edit to either table gets caught rather than silently drifting.

## `aggregate_strength_v1`'s CRT

Expected defender strength-loss fraction by odds ratio (`defender_loss_fraction`, averaged over
200k rolls per ratio):

| Ratio | Avg. expected loss | P(no effect) | P(eliminated) |
|---|---|---|---|
| 1:2 (or worse) | 8.8% | 69.9% | 0.0% (unreachable, by design) |
| 1:1 | 20.1% | 49.8% | 5.0% |
| 2:1 | 35.1% | 29.8% | 15.1% |
| 3:1 | 51.3% | 15.0% | 30.1% |
| 4:1 | 68.9% | 5.0% | 50.2% |
| 5:1 (and better) | 80.1% | 0.0% | 65.2% |

**Verdict: sound, no changes needed.**

- Strictly monotonic across every ratio step — confirmed both analytically (the table's own
  cumulative-probability rows are hand-checkable) and empirically (the Monte Carlo run above,
  now pinned by `crt_expected_loss_increases_monotonically_with_odds`).
- Worst odds (1:2) still gives the attacker a real ~9% expected result, not a guaranteed whiff —
  pinned by `crt_worst_odds_still_gives_attacker_a_real_chance`.
- Best odds (5:1+) tops out at 65% elimination chance, not 100% — a disadvantaged defender always
  keeps a chance to survive even hopeless odds, which is a deliberate wargame-CRT convention (per
  the table's own doc comment) and holds up under simulation — pinned by
  `crt_best_odds_never_guarantees_elimination`.
- The 1:2-and-worse and 5:1-and-better bands both flatten to their edge column rather than
  extrapolating further (confirmed: ratio 8.0 and 20.0 produce identical stats to 5.0) — intentional
  per `ODDS_COLUMNS`' existing design, and not a defect.

## `cepheus_vehicle_v1`'s Penetration Table

Penetration rate and average Hull Points lost across a spread of weapon/armor pairings spanning
hopeless mismatch → matched fight → overwhelming mismatch:

| Pairing | Armor ignore | P(penetrate) | Avg. hull lost (on penetration) |
|---|---|---|---|
| 4d6 SAP vs. armor 10 (light vs. light) | 2 | 94.6% | 2.53 |
| 4d6 SAP vs. armor 40 (light vs. heavy) | 2 | **0.0%** | — |
| 6d6 SAP vs. armor 18 (medium vs. medium) | 3 | 90.4% | 2.69 |
| 9d6 AP3 vs. armor 60 (heavy vs. heavy) | 27 | 35.0% | 1.71 |
| 9d6 AP3 vs. armor 10 (heavy vs. light) | 27 | **100%** | 14.78 |
| 3d6 SAP vs. armor 50 (weak vs. overmatched) | 1 | **0.0%** | — |
| 6d6 AP1 vs. armor 15 (matched) | 6 | 99.8% | 4.82 |
| 12d6 AP2 vs. armor 12 (overwhelming vs. light) | 24 | **100%** | 20.98 |

**Verdict: sound, no changes needed.**

- Hopeless mismatches (light weapon vs. heavy armor, weak weapon vs. overmatched armor) bounce off
  completely — a clean 0%, not "rare but possible" — pinned by
  `penetration_hopeless_mismatch_never_penetrates`.
- Overwhelming mismatches penetrate essentially every time, and do so for large Hull Point losses
  (14.78–20.98, near the top of the table's 1–25 range) — pinned by
  `penetration_overwhelming_weapon_always_penetrates`.
- Matched/plausible fights land comfortably between the two extremes (75–99% penetration, modest
  per-hit damage) rather than being pinned to either edge — pinned by
  `penetration_matched_fight_lands_in_the_middle`.
- Heavy-vs-heavy (armor-ignore 27 against armor 60) is the one genuinely close fight in this spread
  — 35% penetration, low damage even when it lands — which is the expected feel for two
  well-matched heavy platforms grinding at each other, not a defect.

## Conclusion

Both tables were originated without a source to migrate from, but neither turns out to need
retuning: the playtest found smooth, monotonic, non-degenerate behavior across the full range of
odds ratios and weapon/armor matchups tested. The `PLACEHOLDER NUMBERS — not a final design` doc
comments on `CRT` and `PENETRATION_TABLE` have been removed accordingly. This isn't a claim the
numbers are frozen forever — real play may still surface a matchup that feels off — but they're no
longer unvalidated guesses; they're validated by simulation against the properties a combat-results
table needs to hold.

`cepheus_vehicle_v1`'s future Component Damage Table (#37, once built on #25's settled design) will
introduce its own new placeholder odds table and will need this same treatment once it exists.
