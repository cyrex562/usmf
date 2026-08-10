# Combat resolver research: mining `old/` for reusable numbers

Tracks issue #15 (part of the combat-resolver milestone, #11). `design_doc.md` §8 flagged this
back when pluggable rule sets were still an open question: "the old spreadsheets in `old/`
suggest this project has iterated through several rule systems before." This document is that
skim, done before `aggregate_strength_v1` (#16) and `cepheus_vehicle_v1` (#17) author their actual
numeric tables from scratch.

## Bottom line

**Nothing in `old/` is reusable for either resolver.** Both `#16`'s combat-power-ratio CRT and
`#17`'s armor/penetration/hull-points values need to be sourced or originated elsewhere — they are
not buried in this legacy archive. This is a real, checked answer, not an assumption: all 68 files
were surveyed (duplicates collapsed by exact byte size first — most of the 68 are identical copies
of ~25 unique documents under different filenames/timestamps).

## What's actually in there

The `old/` corpus is overwhelmingly:
- **ORBAT / unit-structure trees** — echelon compositions (fireteam → army group), rank tables,
  branch/MOS taxonomies, named real-world unit rosters (1st AD, 1st CD, aviation regiments, etc.,
  some lifted directly from Wikipedia).
- **Equipment-abbreviation legends** (`Ar` = Armour, `AA` = Anti-Armor, `Hwz` = Howitzer) — a key
  for reading unit-type codes, not a stat table.
- **Solar-system reference data** — `harsh_realm_tables.xlsx` and `SystemControlTables.xlsx`
  sounded like game-mechanic tables from their names, but both are orbital-mechanics reference
  data (planet aphelion/perihelion/period/gravity) for a star-map UI, not gameplay tables.

None of that overlaps with what #16/#17 need, so there's nothing to conflict with either — this
isn't a case of "found a different formula and picked one," there simply isn't a competing formula
anywhere in the archive.

## The one combat-mechanic finding

`harsh_matrix_design_doc.docx` (design prose for an earlier "Harsh Matrix" stack-sim layer) is the
only file in the whole corpus that describes any combat-resolution logic at all, in a "Battle
Sequence" section:

> Determine initiative → Take turns → Detect other units → Unit uses action(s): Move, attack,
> special, none, defend
> - If moving, unit's DEF + 1
> - If defending, unit's DEF × 2
> - If attacking, unit's DEF – 1
>
> Attacking: Designate target → Calculate attack value vs target → Apply modifiers (morale,
> strength, command points, logistical points, special) → Calculate target's defense value →
> Subtract target defense value from attack → Subtract attack from target strength

It also sketches a unit-trait schema (ATK/DEF/STR/MOR/INI/DET/STL) where combat is a flat
`ATK − DEF = damage`, `STR -= damage` subtraction.

This is interesting as design history but **not usable data**: no dice, no CRT lookup table, no
ratio-based odds, no armor-penetration mechanic, no hull-points concept, and critically — no actual
numbers anywhere (no worked example, no stat values to seed a table from). It's also a genuinely
different paradigm from what's already been designed for `aggregate_strength_v1` (flat ATK−DEF
subtraction against absolute Strength, vs. a ratio-indexed CRT lookup) — not a compatible alternate
source to adapt, just a different mechanic entirely.

## Method

Read all ~25 unique-content files (after collapsing exact-byte-size duplicate groups — see the
size groups noted in issue #15/#11's history) with format-appropriate tooling (openpyxl/xlrd for
xlsx/xls, python-docx for docx, direct XML for the one `.ods`, PyMuPDF for the two PDFs), then
keyword-scanned every spreadsheet for combat-mechanic terms (`d6`/`d10`/dice/roll/armor/penetrat/
hull point/damage/CRT/combat result/attack value/defense value/kill/elim/casualty/sap/ap\d/odds,
etc.). Files the scan flagged were read in full; everything else was sampled (sheet names +
representative rows) to confirm it matched the ORBAT/reference-data pattern rather than assuming.
The word "armor"/"armour" appears exactly twice in the entire corpus, both as abbreviation-legend
entries, not game-mechanic values.

## Implication for #16 / #17

Both resolvers' actual numbers are original work, not a data-migration exercise:
- **`aggregate_strength_v1`**: author a CRT (odds ratio → result row) from scratch, or adapt one
  from a public tactical-wargame reference if the project wants precedent — not from this archive.
- **`cepheus_vehicle_v1`**: the Hull Points formula, armor faceting question, and full Penetration
  Table remain exactly as open as they were when `cepheus_vehicle_v1` was first scoped (issue #17)
  — this research didn't resolve any of them. #17 stays blocked on those questions; this document
  just confirms `old/` isn't where the answer was hiding.
