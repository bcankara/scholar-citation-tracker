# OJP data (bundled)

`ojp-2024.csv.gz` is compiled into the app (`include_bytes!` in `src/ojp.rs`). It is never downloaded at run time.

| | |
|---|---|
| Edition | 2024 (citations made in 2024 to articles and reviews published 2019-2023) |
| Source dataset | Weitzel, U. (2026). Opindx, dataset version 2026-09-v5, DOI 10.5281/zenodo.23599869, CC BY 4.0 |
| Source file | `scores_2024.csv`, SHA-256 `788de513f9d69f0418aa5798e755314b1fcba2312c2f419beac1531d8f0a1ad4` (Zenodo md5 `f36b5f25c7804ca7dcacefc6c120fc5c`) |
| Shipped file | `ojp-2024.csv.gz`, SHA-256 `034462a329917013c8f250e30762dc5b99ce88664bcd790b2193f36414c04e1b` |
| Rows | 111,087 journals: 85,948 with a score, 25,139 listed without one (`reason` = `few` or `unscored`) |
| Columns | `openalex_id, issn_l, issns (";" separated), title, field, ans, ojp, n, edition, variant, reason` |

Only OpenAlex-universe columns of the source were read (`per_article_oa_raw` and the journal identification
columns). No column derived from the Norwegian Register is read or shipped.

`NOTICE.txt` is the attribution and the statement of changes (CC BY 4.0 section 3(a)); the app shows it on the
Credits page (`get_ojp_notice`). `build_ojp.py` rebuilds the file from the pinned source file
(`python -I build_ojp.py [path/to/scores_2024.csv]`; it refuses an input whose SHA-256 differs from the pin, and its
output is byte-identical on every run). `build_stats.json` holds the counts of that build.

A new edition is a new app release: replace the files, update the constants at the top of `src/ojp.rs`
(a test checks them against the data) and the checksums above.
