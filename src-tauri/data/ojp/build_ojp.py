#!/usr/bin/env python3
"""Build the shipped OJP (Open Journal Prestige) data file from the pinned Opindx release.

Deterministic and idempotent: same input -> byte-identical output (gzip mtime=0, sorted rows,
integer arithmetic for the percentile). Standard library only. Run with:  python -I build_ojp.py

Input  (pinned): Opindx 2026-09-v5, DOI 10.5281/zenodo.23599869, file scores_2024.csv
Output: out/ojp-2024.csv.gz (ranked rows plus unranked rows with empty ojp and a reason)  (or .csv if small), out/NOTICE.txt, out/build_stats.json

Only OpenAlex-universe columns are read. Norwegian Register derived columns (norwegian_*, in_n,
share_n_*, per_article_n_*, citations_n_*) are never read or written.
"""
import csv, gzip, hashlib, io, json, os, sys
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
INPUT = os.path.join(HERE, "opindx_raw_v5", "scores_2024.csv")
OUTDIR = os.path.join(HERE, "out")

# ---- pinned release ------------------------------------------------------------------------
PIN_DOI = "10.5281/zenodo.23599869"          # Opindx dataset version 2026-09-v5
PIN_VERSION = "2026-09-v5"
PIN_FILE = "scores_2024.csv"
PIN_SHA256 = "788de513f9d69f0418aa5798e755314b1fcba2312c2f419beac1531d8f0a1ad4"
EDITION = "2024"          # citing year t=2024; publication window 2019-2023; "freeze-eligible"
VARIANT = "oa_raw"        # OpenAlex universe, Raw treatment (column per_article_oa_raw)
ANS_COL = "per_article_oa_raw"

# ---- rules ---------------------------------------------------------------------------------
MIN_PUBS = 20             # publications_raw (articles/reviews, t-5..t-1) needed for a stable ANS
MIN_FIELD_N = 20          # fields with fewer ranked journals get no OJP
UNKNOWN_FIELD = "Unknown"
NULL = chr(92) + "N"      # Opindx CSV null marker


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def percentile_int(less, equal, n):
    """Mid-rank percentile, rounded half up, exact integer arithmetic.

    ojp = round_half_up(100 * (L + E/2) / n), where, inside one field of n ranked journals,
    L = journals with strictly lower ANS, E = journals with identical ANS (the journal itself included).
    Ties therefore share one score; a field of all-equal values maps to ~50; result is within 0..100.
    """
    return (100 * (2 * less + equal) + n) // (2 * n)


def main():
    inp = sys.argv[1] if len(sys.argv) > 1 else INPUT
    if sha256(inp) != PIN_SHA256:
        sys.exit("input sha256 does not match the pinned release file")

    stats = defaultdict(int)
    kept = []
    unranked = []
    with open(inp, encoding="utf-8-sig", newline="") as f:
        for r in csv.DictReader(f):
            stats["rows_in"] += 1
            if r["score_year"] != EDITION:
                sys.exit("unexpected score_year")
            if r["in_oa"] != "true":
                stats["drop_not_in_oa_universe"] += 1
                continue
            nn0 = lambda s: "" if s == NULL else s
            base = [r["openalex_id"], nn0(r["issn_l"]), nn0(r["issns"]).replace("; ", ";"), r["title"]]
            # journals left without a score are still shipped (reason few / unscored), so the app
            # can tell "known but not ranked" from "not in the data"
            if int(r["publications_raw"]) < MIN_PUBS:
                stats["drop_below_min_pubs"] += 1
                unranked.append(base + [nn0(r["oa_field"]), nn0(r[ANS_COL]), "", "", EDITION, VARIANT, "few"])
                continue
            if r[ANS_COL] == NULL:
                stats["drop_ans_undefined"] += 1
                unranked.append(base + [nn0(r["oa_field"]), "", "", "", EDITION, VARIANT, "unscored"])
                continue
            fld = r["oa_field"]
            if fld in (NULL, "", UNKNOWN_FIELD):
                stats["drop_unknown_field"] += 1
                unranked.append(base + ["", r[ANS_COL], "", "", EDITION, VARIANT, "unscored"])
                continue
            kept.append((r["openalex_id"], r["issn_l"], r["issns"], r["title"], fld, float(r[ANS_COL]), r[ANS_COL]))

    by_field = defaultdict(list)
    for row in kept:
        by_field[row[4]].append(row[5])

    rank = {}
    for fld, vals in by_field.items():
        vals.sort()
        n = len(vals)
        if n < MIN_FIELD_N:
            continue
        first = {}   # value -> (count strictly less, count equal)
        i = 0
        while i < n:
            j = i
            while j < n and vals[j] == vals[i]:
                j += 1
            first[vals[i]] = (i, j - i)
            i = j
        rank[fld] = (n, first)

    out_rows = []
    for sid, issn_l, issns, title, fld, ans, ans_txt in kept:
        if fld not in rank:
            stats["drop_small_field"] += 1
            unranked.append([sid, issn_l if issn_l != NULL else "", issns.replace("; ", ";") if issns != NULL else "", title, "", ans_txt, "", "", EDITION, VARIANT, "unscored"])
            continue
        n, first = rank[fld]
        less, eq = first[ans]
        nn = lambda s: "" if s == NULL else s
        out_rows.append([sid, nn(issn_l), nn(issns).replace("; ", ";"), title, fld, ans_txt,
                         percentile_int(less, eq, n), n, EDITION, VARIANT])
    out_rows = [r + [""] for r in out_rows]
    all_rows = out_rows + unranked
    all_rows.sort(key=lambda x: x[0])

    os.makedirs(OUTDIR, exist_ok=True)
    buf = io.StringIO()
    w = csv.writer(buf, lineterminator="\n")
    w.writerow(["openalex_id", "issn_l", "issns", "title", "field", "ans", "ojp", "n", "edition", "variant", "reason"])
    w.writerows(all_rows)
    data = buf.getvalue().encode("utf-8")
    ext = "csv"
    if len(data) > 3 * 1024 * 1024:
        ext = "csv.gz"
        raw = io.BytesIO()
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0, compresslevel=9) as gz:
            gz.write(data)
        data = raw.getvalue()
    for old in ("ojp-%s.csv" % EDITION, "ojp-%s.csv.gz" % EDITION):
        p = os.path.join(OUTDIR, old)
        if os.path.exists(p):
            os.remove(p)
    outpath = os.path.join(OUTDIR, "ojp-%s.%s" % (EDITION, ext))
    with open(outpath, "wb") as f:
        f.write(data)

    dist = [0] * 101
    for r in out_rows:
        dist[r[6]] += 1
    st = dict(stats)
    st.update(rows_out=len(out_rows), rows_unranked=len(unranked), rows_shipped=len(all_rows), fields=len(rank), field_sizes={k: v[0] for k, v in sorted(rank.items())},
              fields_dropped_small=sorted(k for k in by_field if k not in rank),
              output_file=os.path.basename(outpath), output_bytes=len(data),
              output_sha256=hashlib.sha256(data).hexdigest(), ojp_histogram_by_10=[sum(dist[i:i + 10]) for i in range(0, 100, 10)],
              ojp_100=dist[100], ojp_0=dist[0])
    with open(os.path.join(OUTDIR, "build_stats.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(st, f, indent=1, sort_keys=True)
        f.write("\n")

    notice = f"""OJP (Open Journal Prestige) data - attribution notice

This file contains data derived from:
  Weitzel, Utz (2026). Opindx: Open Journal Factor, Network Factor and Article Network Score
  (dataset version {PIN_VERSION}) [Data set]. Zenodo. https://doi.org/{PIN_DOI}
  Licensed under Creative Commons Attribution 4.0 International (CC BY 4.0):
  https://creativecommons.org/licenses/by/4.0/
  Original file used: {PIN_FILE} (SHA-256 {PIN_SHA256}), citing year {EDITION}.
  Opindx numerical inputs for the columns used: OpenAlex (CC0), https://openalex.org

Changes made to the original material (CC BY 4.0 section 3(a)(1)(B)):
  - Only the OpenAlex-universe, Raw-treatment Article Network Score (column {ANS_COL}) and journal
    identification columns (openalex_id, issn_l, issns, title, oa_field) were kept; all other columns were dropped.
  - All columns derived from the Norwegian Register (norwegian_*, in_n, share_n_*, per_article_n_*,
    citations_n_*, norwegian_entries_json, norwegian_primary_id) were NOT used and are not included.
  - Journals with undefined ANS, fewer than {MIN_PUBS} articles/reviews in the five-year window, field "{UNKNOWN_FIELD}",
    or in a field with fewer than {MIN_FIELD_N} ranked journals were left without a score (reason few / unscored; {len(out_rows)} journals have a score,
    {len(unranked)} are listed without one).
  - A new value, ojp (0-100), was computed: the within-field mid-rank percentile of ANS,
    ojp = round_half_up(100 * (L + E/2) / n), L = journals with lower ANS in the same field,
    E = journals with equal ANS (including itself), n = ranked journals in the field.
  - Whitespace after ';' in the ISSN list was removed.
The author of Opindx did not review or endorse this derived work. OJP is not an official Opindx metric.
"""
    with open(os.path.join(OUTDIR, "NOTICE.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write(notice)
    print(json.dumps({k: v for k, v in st.items() if k != "field_sizes"}, indent=1, sort_keys=True))


if __name__ == "__main__":
    main()
