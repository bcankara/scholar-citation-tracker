# Scholar Citation Tracker

<p align="center"><img src="docs/images/banner.png" alt="Scholar Citation Tracker" width="100%"></p>

<p align="center"><a href="https://github.com/bcankara/scholar-citation-tracker/releases/latest"><b>⬇ Download for Windows or macOS</b></a> · <a href="README.tr.md">Türkçe</a></p>

A small desktop app for **Windows and macOS** (Rust + Tauri 2) that tracks the citation counts on a public Google Scholar profile over time. It is meant for personal use: you point it at your own profile, it checks the page at a low rate, and it keeps the history on your computer.

The interface is available in **English** and **Turkish**. On first launch the language follows the display language of the operating system; you can change it at any time in Settings.

## Highlights

- Reads your public Scholar profile at a low rate and keeps the history (total, h-index, i10-index, per-article counts) in a local SQLite database.
- A dashboard that says in one sentence what changed since your last visit.
- A calendar of Scholar changes and, with historical data on, of citing works by exact publication date, plus a heatmap of all years.
- A continuous trend line with a year-end forecast range and a goal, the fastest-rising articles, and h / i10 over time.
- A "Citing works" analysis by country (world map), institution, venue and publisher, field, publication type, open access and language, who cites you (self, co-author, external) and influential citations.
- A 0-100 **OJP** journal score next to every citing work whose journal is known (sortable, in the Analysis and in the exports), a Help page that explains it, and a Licences and credits page.
- A filterable list of the citing works (title linked to DOI/OpenAlex); each article and each Scholar change links to that article's cited-by list on Scholar.
- A PDF report with selectable charts, an Excel export and a weekly summary notification.
- English and Turkish interface, light and dark theme, in-app signed updates.

## Tour

All screenshots show invented demonstration data.

### Dashboard

One sentence says what changed since your last visit. Below it: the four totals, the latest changes grouped by day and a 12-month history.

<img src="docs/images/en-panel.png" alt="Dashboard with a sentence on what changed, four totals, recent changes and a 12-month history" width="100%">

### Analysis: Calendar and heatmap

A month grid marks the days on which Scholar showed a change and, with historical data, the days on which citing works were published. Selecting a day lists the articles and the citing works behind it. The heatmap shows all years at once.

<table>
<tr>
<td width="50%"><img src="docs/images/en-calendar.png" alt="Calendar card showing November 2026 with one day selected and its Scholar changes listed" width="100%"></td>
<td width="50%"><img src="docs/images/en-heatmap.png" alt="Heatmap of busy days, year by year, for all years" width="100%"></td>
</tr>
</table>

### Analysis: Trend

One continuous cumulative line, a forecast to the end of the year drawn as a range, your goal, and citations per period.

<img src="docs/images/en-trend.png" alt="Analysis Trend tab with a continuous line, forecast range, goal and citations per period" width="100%">

### Analysis: Articles

The fastest-rising articles, the gap per article between Scholar and identified citing works, and monthly citations stacked by article.

<img src="docs/images/en-analysis-articles.png" alt="Analysis Articles tab with fastest rising articles, Scholar against identified citations and a stacked monthly chart" width="100%">

### Analysis: Indices

The h-index and i10-index as step lines over time, and the articles closest to the next step.

<img src="docs/images/en-indices.png" alt="Analysis Indices tab with h and i10 over time and the articles closest to the next step" width="100%">

### Analysis: Citing works

This tab answers "who is citing me?" with one card per category. One period selector, publication filter and "Exclude self-citations" switch apply to the whole tab, and each chart states for how many works the data is missing.

#### Countries

World map, top 10 countries, countries per year and countries citing for the first time.

<img src="docs/images/en-citers-countries.png" alt="Citing works tab with a filter bar and a Countries card showing a world map and the top 10 countries" width="100%">

#### Institutions

Top 10 institutions, with new ones highlighted.

<img src="docs/images/en-citers-institutions.png" alt="Institutions card with the top 10 institutions and those new this year" width="100%">

#### Venues and publishers

The journals and publishers of the citing works.

<img src="docs/images/en-citers-venues.png" alt="Venues and publishers card" width="100%">

#### Journal prestige (OJP)

How the citing works are spread over the OJP score of their journal, and the top citing journals with their scores.

<img src="docs/images/en-citers-ojp.png" alt="Journal prestige card with a ten-step histogram from white to red and a table of the top 8 citing journals with OJP badges" width="100%">

#### Fields

A treemap of fields and a heatmap of your publications against fields.

<img src="docs/images/en-citers-fields.png" alt="Fields card with a treemap and a publication by field heatmap" width="100%">

#### Publication types

Type, open access and language of the citing works.

<img src="docs/images/en-citers-types.png" alt="Publication types card with bars for type, open access and language" width="100%">

#### Who is citing you

The share of self-citations (matched by ORCID and name), co-author citations and external citations, and the most frequent citing authors.

<img src="docs/images/en-citers-who.png" alt="Card separating self, co-author and external citations, with frequent authors" width="100%">

#### Influential citations

A table of the influential citing works.

<img src="docs/images/en-citers-influential.png" alt="Influential citations table" width="100%">

### Citing works list

Clicking a bar in a chart filters the list. Active filters appear as chips that can be removed one by one or all at once.

<img src="docs/images/en-citing.png" alt="Citing works page with charts by country and venue, removable filter chips and a list of 11 of 200 works" width="100%">

The OJP column shows the score of each citing work's journal as a badge; click its header to sort (works without a score always come last), hover a badge for the field and the reason when there is none.

<img src="docs/images/en-citing-ojp.png" alt="Citing works list sorted by journal OJP, with badges from white to red" width="100%">

### Articles

Each article opens to its history chart and every change in its count, the matched OpenAlex record (with a way to correct the DOI) and the works citing it.

<img src="docs/images/en-articles.png" alt="Articles page with the first article expanded: history chart, change history, OpenAlex match and citing works" width="100%">

### Report (PDF) and Excel

Choose the range, the language and the charts for a PDF; a live preview shows them. The Citing-works sections (countries and institutions, fields, self-citation) are optional. The Excel export is in Settings.

<img src="docs/images/en-report.png" alt="Report page with the optional Citing-works sections ticked and shown in the live preview" width="100%">

### Settings and updates

Settings holds the profile, interval, language, data folder, API keys, historical data and export. About shows the version and checks for updates; the result is always shown.

<img src="docs/images/en-settings-about.png" alt="Settings scrolled to About, showing that the app is up to date" width="100%">

### Help, and Licences and credits

The **Help** page explains OJP (what it is, how it is computed, what it is not) and has short notes on the other pages. **Settings > About > Licences and credits** lists the data sources with their licences, the bundled components and every Rust library compiled into the app, each with its licence text.

<img src="docs/images/en-help.png" alt="Help page: what OJP is and how to read the badge" width="100%">

<img src="docs/images/en-credits.png" alt="Licences and credits page: application, OpenAlex and the OJP dataset with its attribution" width="100%">

### Dark theme

The interface follows the Windows light/dark setting.

<img src="docs/images/en-dark-citers.png" alt="Dark theme showing the Countries card with a world map" width="100%">


## What it does

- Reads the profile page at a fixed interval (default: every 3 hours, configurable from 1 to 24) and stores total citations, h-index, i10-index, per-article citations and the yearly citation histogram in a local SQLite database.
- The **Dashboard** says in one sentence what changed since your last visit, shows four totals (the h and i10 cards also say what the next step needs, for example "3 more citations for h=9"), the latest changes grouped by day and a 12-month citation history. Trends, forecasts and breakdowns live on the **Analysis** page, the PDF on the **Report** page; see [Dashboard, Analysis and Report](#dashboard-analysis-and-report). The Articles tab lets you open the history of each article, including every change of its count, up or down.
- Shows a system notification when a citation count rises or a new article appears (the very first fetch is silent). Scholar sometimes removes citations (and may add them back later): decreases are shown in red in the tables and the change history, but never trigger a notification. If one check has both, the notification shows the net change and the articles that rose.
- Failed attempts are retried every 5 minutes, up to 3 more times; every attempt is listed in the Fetch log tab. If Google blocks the request (HTTP 429 / 403, CAPTCHA page), the attempt is recorded as "Blocked" and the app waits for the next cycle.
- Closing the window hides the app in the notification area on Windows, in the menu bar on macOS. That menu has "Open dashboard", "Refresh now", "Open data folder" and "Quit". On Windows a left click on the icon opens the dashboard and the menu belongs to the right button; on macOS a left click opens the menu, as it does for every other menu-bar item.
- Optional start at sign-in (Windows) or at login (macOS, as a Launch Agent): the app then starts hidden, in the tray or menu bar only. It is on by default but is only registered once you have entered a profile, and you can turn it off in Settings.
- The summary history and the article history can be exported as CSV (UTF-8 with BOM, so Excel shows non-ASCII characters correctly), or everything as one Excel workbook. Column headers follow the selected language.
- Nothing is fetched until you enter a profile. The first-run card accepts either a bare profile ID or the full profile URL (`https://scholar.google.com/citations?user=XXXX&hl=en`), together with your ORCID iD (required, see [Historical data](#historical-data-optional)).
- Changing the profile later deletes the stored data of the previous profile, after a confirmation.
- Optional [historical data](#historical-data-optional): past citations by day, month and year from OpenAlex, matched through your ORCID iD.
- A list of the [works that cite you](#citing-works-forecast-reports-and-export), a calendar of citations by exact date, a forecast and goal estimate, the fastest-rising articles and the next h / i10 step, a PDF citation report with selectable charts, an Excel export and a weekly summary notification.
- An optional OpenAlex [API key](#api-keys-optional) for faster, less rate-limited lookups.

## Dashboard, Analysis and Report

**Dashboard (what changed).** Under the profile name one sentence says what happened *since your last visit* (the moment the window was last seen, compared with the snapshot of that moment). Priority: an index threshold crossed ("your h-index rose to 8"); new citations and the article behind most of them; a net decrease, stated neutrally; closeness to the next h. When nothing changed it says so, and when the last rise was. Article names and the h phrase are links. Below: four totals (the h and i10 cards also say what the next step needs, e.g. "3 more citations for h=9"), the recent changes (at most six, grouped under date headings; a date opens the Calendar on that day) and a compact 12-month history line.

**Analysis (trends, forecast, calendar).** A period selector (*Last 30 days*, *Last 90 days*, *Last 12 months*, *All*, *Custom*) applies to the charts; *Report this period* opens the Report page with that range. Grouping (*Daily / Monthly / Yearly*) is a separate, labelled control. Four tabs; each chart is titled with the question it answers:

- **Calendar** (the default tab): a month grid in the style of the Windows CalendarView. Days with activity are shaded by count; a blue dot marks changes **Scholar** showed that day (dated by the day it showed them), a teal dot marks citing works with an exact publication date (**OpenAlex**), a muted red dot a day with only decreases. Click a day (Shift-click a second one for a range, or "Whole month") and the panel on the right lists which of your articles gained or lost citations that day (with "View on Scholar") and which citing works were published that day. Arrow keys, Home / End and PageUp / PageDown move around the grid, Enter selects. Filters: source (All / Scholar / OpenAlex) and article. Works dated only by month or year are never put on a day; they are in a separate "Date unknown (N)" list. Below the month: a year heat map (a click jumps to that day) and the cumulative history line.
- **Trend**: one continuous cumulative line. The history before the first Scholar check comes from OpenAlex (citing works by publication date) and is scaled proportionally so that its last point equals the first Scholar value (no gap, no jump; a thin guide marks "Scholar tracking started"). After that the line follows the Scholar checks. A forecast to the end of the year continues in purple with a shaded range; it stays hidden until the checks cover 60 days. The rate comes from 30-day windows between snapshots, so the first scan (everything Scholar had already counted) is never counted as growth. Below: the year-end estimate as a range, the monthly average, the last 12 months, the goal, and citations per period (click a bar to open its dates in the Calendar).
- **Articles**: the fastest-rising articles, the gap between Scholar and identified citing works per article, and monthly citations stacked by article (top five and "Other").
- **Indices**: h and i10 as step lines over the checks, and the full list of articles closest to the next h and i10.

Every caption names the date it uses: *the day Scholar showed the change* (the checks) or *the publication date of the citing work* (OpenAlex). The two are never mixed on one axis without saying so.

**Report (the PDF).** Choose the range, the language and which charts to include (trend, per period, per article, indices, calendar, country and venue). A small live preview shows the same charts. The PDF is drawn with the same chart code as the screen, as vector graphics, so the numbers match.

### Scholar and OpenAlex do not count the same, and OpenAlex is late

Scholar counts more and earlier; OpenAlex adds new papers with a lag of several weeks and does not know some works. The app shows the gap instead of hiding it:

- **Per article:** "Scholar 59 · identified 40 · not yet identified 19" with a thin two-part bar (Articles tab and Analysis > Articles).
- **Fastest rising** states its basis. Scholar snapshot gains come first. If the snapshot history is shorter than the window it says so ("Scholar tracking is 3 days old: +5 in that time") and lists OpenAlex-based items separately, labelled "According to OpenAlex (date of the citing work)". It always shows "Newest citing work in OpenAlex: <date>", and when there is nothing to show it says why (no Scholar history yet, and/or no OpenAlex records after that date) and what to try.
- **The lag tail** (days after the newest OpenAlex citing work) is lightly hatched in the Calendar and in the per-period bars with "may not be indexed yet"; it does not mean zero citations. OpenAlex-based rates ignore the months after the newest citing work.
- **Probable attribution.** A Scholar increase (article, day, +n) is linked, greedily and one-to-one, to citing works of the same article whose publication date lies 45 days before to 15 days after the event, closest first. The day detail shows them as nested "probably: <title> · <venue>"; increases without a match stay "Seen on Scholar — details not available yet". Decreases are never matched. The links are derived from the data each time (nothing is stored), so they fill in by themselves as OpenAlex catches up. They are guesses and are always labelled as such.

## Historical data (optional)

Scholar data is only collected from the day the app is installed. If you want, the app can fill in the past from free sources, at day, month and year level. It is optional and off until you choose it: a dialog offers it after the first-run setup and after a confirmed profile change ("Look up history too" / "Start from today"), and **Settings > Historical data** has the switch, a "Run now" button, a status line (last run, matched articles, citing records) and "Delete historical data".

- **ORCID iD (required).** The first-run card and Settings ask for your ORCID iD next to the Scholar profile. Enter `0000-0002-1825-0097` or `https://orcid.org/0000-0002-1825-0097`; the format and the ISO 7064 mod 11-2 check digit are validated, and "Start" stays disabled until both fields are valid. If the saved settings have no ORCID iD, the setup card appears and asks only for it; Scholar tracking keeps running meanwhile, only the historical lookup needs it. Changing the ORCID iD later deletes the historical data, because the matches depend on it.
- **Matching.** The ORCID iD resolves to an OpenAlex author. The author's works are collected from `author.orcid:<iD>` and `authorships.author.id:<author id>` (union by work id). Each Scholar article is assigned to the best candidate in that set: title similarity (Sørensen–Dice on word tokens) of at least 0.80 and a publication year within one year of Scholar's. A work is used for at most one article; near-duplicates such as an article and its later "Version 2" record are told apart by similarity first and the year second. Articles not found in that set fall back to a global OpenAlex title search, with the stricter threshold of 0.85. How each match was made (`orcid`, `title_search` or `manual_doi`, see [Correcting a historical match](#correcting-a-historical-match-doi)) is stored, and articles that match nowhere are shown as "not matched".
- **Source.** For every article OpenAlex is asked for the works that cite it. The expanded row in the Articles tab shows the count, for example "OpenAlex 40", the match method, and links to the OpenAlex record and the DOI. OpenAlex alone is used because Semantic Scholar and OpenCitations were compared on real articles and added at most a couple of citing works that OpenAlex did not already list, while Semantic Scholar's strict rate limit made scans several times slower; Crossref only gives a total without citing dates (its cited-by list is limited to members).
- **Dating rule.** A citation is dated by the publication date of the citing work: the full day when the source has it, otherwise the month or the year. Records with only a year appear in the Year view and are left out of the Day / Month views (the chart says how many).
- **Counts differ from Scholar.** The sources index different works than Google Scholar, so totals will not match; nothing is reconciled, and every chart names its source. The one exception is the Trend line, where the history before the first Scholar check is scaled proportionally to the first Scholar value (the shape is kept; the chart says so). Per-period bars use the citing works (OpenAlex) at day, month or year level; without historical data only the Scholar year histogram is available.
- **Running.** It runs in the background after the first Scholar check (or right away if data exists), one run at a time, and never blocks or affects the Scholar scheduler; errors are recorded in the settings card. When a later check shows a new article, it is looked up automatically. Re-running replaces the stored records instead of adding duplicates. Without an API key OpenAlex is called about once per second (with backoff on HTTP 429); with a key about 0.4 s apart. A long article list still takes a while, so the run is shown in a [scan panel](#scan-panel) and can be stopped.
- **Privacy.** Requests carry a generic User-Agent (`scholar-citation-tracker/<version> (+repository URL)`) and no e-mail address. Article titles, DOIs and your ORCID iD are sent to OpenAlex; nothing is sent to Google for this feature.

## Scan panel

While a historical scan runs, a panel slides in from the right edge (**Historical scan**; close it with the X, reopen it from the header pill "Scanning history 3/11" or from Settings). It opens by itself when you start the scan yourself (the first-run choice, "Run now", a DOI correction); the automatic daily refresh only shows the pill.

- An overall progress bar with "Article 3 / 11", the elapsed time and an estimate of the time left.
- The current step in plain words: "Fetching citing works from OpenAlex — page 2, 87 records", or "OpenAlex rate limit: waiting 12 s" with a live countdown when a server asks the app to slow down (HTTP 429 / 503).
- Every article of the run with its state (queued, running, done, not matched, error) and its result, for example "OpenAlex 40".
- A collapsible **Details** log with the last 30 events and their times.
- **Stop** ends the run after the current request; what was already stored stays, the run is recorded as "stopped".
- When the run ends, a summary (matched X of Y, records, new ones, duration) stays until you close the panel, with the number of articles that were not matched or have no DOI.

On narrow windows the panel takes the full width.

## Recent citations and Scholar's cited-by list

The dashboard's **Recent changes** card merges two kinds of entries into one list, newest first, grouped under date headings (the latest six; "See all" shows more, "Open in Calendar" and a click on a date open Analysis > Calendar):

- **Changes seen on Scholar.** Whenever the count of an article differs between two consecutive stored snapshots, a row such as "21 Nov — <article> +2" is listed with a "Google Scholar" badge; a decrease is listed in red as "Decreased on Scholar". The very first snapshot has nothing before it and is never counted.
- **Citing works** found by the historical lookup (OpenAlex), with title, venue, the cited article and the source badge.

Each Scholar row, and each article in the Articles tab, has **View on Scholar**. It opens Scholar's cited-by list for that article, sorted by date, **in your default browser**. The app does not read that page itself: Scholar's search pages are protected by a CAPTCHA, and the app only ever reads your public profile page at a low rate. The link is built from the `cites=` ids that the profile page already contains for each article.

## Citing works, forecast, reports and export

- **Citing works (Citing works tab).** While historical data is on, the lookup stores for every citing work its title, publication date, venue (journal), DOI, link, authors, the country codes of the authors' institutions and its record id. The tab lists them all (date, title linked to the DOI / OpenAlex page, venue, which of your articles it cites) with a search box, an article filter and date sorting; the expanded row of an article in the Articles tab lists that article's citing works too. Two charts show the top 10 countries and the top 10 venues of the citing works (each work counts once, even if it cites several of your articles); a caption says how many works have no country data. The citing works by year are already part of the history chart on the dashboard. Click a bar of the country or venue chart and the list is filtered to it; active filters (country, venue, date or period from the Calendar, article) are shown as removable chips.
- **New citing works.** While historical data is on, a light refresh runs about once a day (ten minutes after start at the earliest, then whenever 24 hours have passed). It re-reads only the citing lists of articles that are already matched, using the stored OpenAlex id, with the request pacing described below and no re-matching. When it finds citing works that were not stored before (never on a first lookup), a system notification names up to two of them. It follows the "Notifications" setting.
- **Forecast (Analysis > Trend).** One continuous cumulative line with a forecast to the end of the year, drawn as a range. The rate is the average of 30-day windows between Scholar snapshots (newest first, up to six), the range follows how much the windows differ, the first scan is never counted as growth, and the forecast is hidden until the snapshots cover 60 days. A goal you type in gives "N citations expected around <month year>". Always labelled as an estimate; the goal is stored in the local database.
- **Fastest rising (Analysis > Articles).** The five articles that gained the most in the last 30 or 90 days, with its basis stated (see [Scholar and OpenAlex do not count the same](#scholar-and-openalex-do-not-count-the-same-and-openalex-is-late)). What the next h-index and i10-index need is on the Indices tab and on the dashboard cards.
- **PDF report (Report tab).** Choose a date range, the report language (Turkish or English) and whether to include the citing-works list and the charts, then **Create PDF**. The A4 PDF with page numbers contains a title page (profile name, ORCID iD, range, generation date), summary metrics (total, since-year, h, i10, change in the range), a table per article (citations at the end of the range, gained in the range), charts (total over time, per year), optionally the citing works of the range (date, title, venue, cited article) and a sources-and-method note. The report is an HTML page (`ui/report.html`) rendered in a hidden window and printed to PDF by the platform's own webview printing (`PrintToPdf` on WebView2, an `NSPrintOperation` on `WKWebView`), so Turkish characters come from the system font. Both produce the same A4 pages; on macOS the job is sent straight to a PDF file through Core Printing, so no printer has to be configured.
- **Excel export (Settings > Export).** One .xlsx with the sheets Summary (snapshot history), Articles (latest values with today / 7 / 30-day changes), Article history (long format), Citing works and Yearly. Header rows are bold and filled, frozen and filtered; dates are real Excel dates; sheet names and headers follow the interface language. The two CSV exports remain.
- **Weekly summary.** On the first start on Monday at or after 09:00 (local time), once per ISO week, a notification says "This week +N citations; most: <article> (+k)". It is skipped when the week has no data. Switch: **Settings > Weekly summary notification** (on by default; it also needs "Notifications").

### Citing works tab (Analysis)

The last Analysis tab, **Citing works**, answers "who cites me?" one category per card: countries (world map, top 10, countries per year, first-time countries), institutions (new ones highlighted), venues and publishers, fields (treemap and a publication x field heatmap), publication types (type, open access, language), who (self-citation share by ORCID with a name fallback, co-author and external citations, top authors) and influential citing works. The period selector, an article filter and "Exclude self-citations" apply to the whole tab; each chart states how many works lack the data. The data comes from the same OpenAlex requests as before (extra `select` fields); rows stored earlier are filled at the next refresh. Each card has a small "view in list" link; the list page gained type, open-access, field and self-citation filters. The PDF report has three optional sections (countries and institutions, fields, self-citation) and the Excel sheet has 13 new columns.

The year heat map in Analysis > Calendar now shows every year at once.

## OJP journal score

**OJP** (Open Journal Prestige) is a number from 0 to 100 that the app shows next to the journal of every citing work.

**What it is**

- The relative position of a journal among the journals of its own field: 0 is the lowest position, 100 the highest. An OJP of 76 means the journal scores higher than about 76 of every 100 journals of its field.
- Computed from the Article Network Score (ANS) of an open dataset built on OpenAlex data: within each field the journals are ranked by ANS and the rank is turned into a percentile (OJP = 100 x (journals with a lower score + half of the journals with the same score) / journals in the field, rounded). Each journal belongs to one field, the one where most of its articles fall.
- Shown as a badge that deepens from white through yellow to red; the number carries the information and the colour only repeats it. There are no classes or tiers.
- Where it appears: the Citing works list (sortable column), a card in Analysis > Citing works (distribution and top journals), the Excel export (three columns and an "About OJP" sheet) and, optionally, the PDF report.
- Data: the 2024 edition (citations made in 2024 to articles and reviews published in 2019-2023), 85,948 journals in 25 fields. It ships inside the app, nothing is downloaded, and it changes only with a new app version. Rows stored before 1.4.0 are matched by ISSN until the next refresh stores the OpenAlex source id. Notes on the file: `src-tauri/data/ojp/README.md`.

**What it is not**

- It is **not** a Web of Science, Scopus, SJR or JCR indicator and not a Q1-Q4 quartile. It cannot be converted to one and must not be reported in place of one.
- It is not an official metric of the dataset it is derived from; the author of that dataset has not reviewed or endorsed it.
- It describes journals, not articles or people, and must not be used for appointments, promotion, awards, funding or any other formal evaluation of people.
- It is a position within one field; scores of journals in different fields are not directly comparable.
- Journals with fewer than 20 counted articles in the window, without a score or field in the source data, or in fields with fewer than 20 ranked journals get no OJP and show **NA** ("score not calculated yet"): a dashed outline when the journal is known but not ranked, plain NA when it is not in the data, with the reason in the tooltip; works that are not in a journal (books, preprints, theses) have an empty cell. Some software and data journals fall below the threshold.

The data source and its licence are listed under [Licences and credits](#licences-and-credits); the Help page of the app explains the same in more detail.

## API keys (optional)

**Settings > API keys (optional)** takes a free OpenAlex key. The app works without one.

- **OpenAlex.** A key is not required. Without one, the daily budget is shared by IP address (it can run out quickly on university networks) and the limit is 10 requests per second; a free key raises the daily budget 10 times and the limit to 30 requests per second. Get one at [openalex.org](https://openalex.org) (create an account and copy the key from Settings > API key), or press **Request a free key by email**: you type your e-mail address and the app calls `POST https://api.openalex.org/api-keys`; OpenAlex answers with the key (status `pending_activation`), the app stores it and you click the activation link in your e-mail. The address is sent only on that click and is never stored. See the [OpenAlex rate limits and authentication](https://docs.openalex.org/how-to-use-the-api/rate-limits-and-authentication) page. The key is sent as `Authorization: Bearer <key>`, never in the URL.
- **Daily budget spent.** When OpenAlex answers HTTP 429 with the reason `daily_budget`, the app stops the scan cleanly (what is stored stays, no half-read article), marks the run "paused" and shows a notice on the scan panel, in Settings > API keys and as a dismissible bar on the Dashboard, Analysis and Citing works pages, with the time the budget resets (midnight UTC, shown in your local time) and buttons for a free key, an e-mail key request and Settings. The scan resumes by itself a few minutes after the reset (or at the next start); nothing asks OpenAlex before that. A per-second limit just waits for `Retry-After`. A system notification appears at most once a day and only for scans you started.
- **Storage and safety.** Keys are stored in the platform credential store (the Windows Credential Manager, or the macOS Keychain) under the service name `scholar-citation-tracker`, not in `settings.json` or the database. They are never written to logs, run messages or error texts (anything that looks like a key is redacted). The **Test** button makes one cheap request and says whether the key is valid. If OpenAlex rejects the stored key (HTTP 401 / 403), the app warns once and continues without it.

## Where data is stored

- Database: `Documents\ScholarTakip\scholar_takip.db` on Windows, `~/Documents/ScholarTakip/scholar_takip.db` on macOS (SQLite, WAL mode). You can change the folder in Settings; if the new folder has no database the current one is copied there, otherwise the one that is already there is used.
- Settings: `%APPDATA%\io.github.bcankara.scholarcitationtracker\settings.json` on Windows, `~/Library/Application Support/io.github.bcankara.scholarcitationtracker/settings.json` on macOS
- OpenAlex API key (if you add one): Windows Credential Manager or macOS Keychain, service name `scholar-citation-tracker`.

## Correcting a historical match (DOI)

Open an article in the Articles tab and look at the historical-data column. Three actions fix a wrong or missing match:

- **Set DOI.** Enter the article's correct DOI as `10.xxxx/...`, `doi:10.xxxx/...` or `https://doi.org/10.xxxx/...`. The article is then looked up by that DOI only (`https://api.openalex.org/works/doi:<doi>`), with no title matching; the method is shown as "manual DOI". If OpenAlex does not know the DOI, the article is shown as not matched, together with the DOI.
- **Not this one.** Rules the current match out for this article and looks it up again; the rejected OpenAlex record is never offered to this article again.
- **Reset to automatic.** Removes the manual DOI and the rejected records and matches the article automatically again.

Saving any of these immediately re-runs the lookup for that one article only. The corrections are stored in the local database and survive later runs and "Delete historical data" (they are your own decisions, not computed data); they are removed only when you change the Scholar profile, which clears all stored data.

## DOI of your articles

The Articles tab shows a **DOI** chip under each title when a DOI is known (a manual DOI first, otherwise the one of the matched OpenAlex record) and an **Add DOI** button when none is. The button opens the same dialog as "Set DOI" without expanding the row. When some articles have no DOI, an information bar above the table says how many and can filter the list to them (it can be dismissed for the session); the dashboard and the scan summary link to that filtered list. A DOI makes the historical data and the citing works match the right record.

## Health warning

If Scholar could not be read successfully for 2 days in a row (the last successful fetch is at least 48 hours old although attempts were made in that time), the app shows one system notification ("Scholar has been unreadable for 2 days - see the Fetch log") and a warning bar on the Panel until the next successful fetch. The notification is shown once per incident, never more than once every 24 hours, and follows the "Notifications" setting. The same applies when the page is served but its article list cannot be read: if the profile shows citations but no articles, or the article count drops by more than half while the citation total does not drop, the attempt is recorded as a parse error in the Fetch log and no snapshot is stored ("article list could not be read - Scholar's page layout may have changed").

## Installing

### Windows

Download the installer (`Scholar.Citation.Tracker_<version>_x64-setup.exe`) from the **Releases** page and run it. It installs for the current user only and does not need administrator rights. The installer is available in English and Turkish. The app needs the Microsoft WebView2 runtime, which is already part of current Windows 10 / 11.

Run the installer by double-clicking it in File Explorer. Running it from inside a sandboxed or packaged app (for example a chat or store app's file preview) can make Windows virtualize the installation, so it won't appear in Installed apps.

**The installer is not code-signed**, so Windows SmartScreen shows *"Windows protected your PC"* the first time. Click **More info**, then **Run anyway**.

### macOS

Download the disk image (`Scholar.Citation.Tracker_<version>_universal.dmg`) from the **Releases** page, open it and drag the app to **Applications**. One universal build covers both Apple Silicon and Intel Macs; macOS 11 Big Sur or newer is required.

**The app is not signed with an Apple Developer ID and is not notarized**, so Gatekeeper refuses it on the first launch (*"Apple could not verify ... is free of malware"*). Either right-click the app in Applications and choose **Open**, then confirm, or clear the quarantine flag once:

```
xattr -dr com.apple.quarantine "/Applications/Scholar Citation Tracker.app"
```

The app keeps a menu-bar icon; closing the window hides it there rather than quitting. Quit from the menu-bar icon or with Cmd-Q.

### Verifying the download

Why the warnings: a code-signing certificate (and, on macOS, an Apple Developer Program membership) costs money every year, and this is a small personal open-source project without either. Instead, both builds are produced in the open by GitHub Actions from the source in this repository (see `.github/workflows/release.yml`), and you can check that the file you downloaded is the one that workflow built. Replace `<file>` with your download.

1. **SHA-256.** Every release has a `SHA256SUMS-windows.txt` and a `SHA256SUMS-macos.txt`. Compare the matching one with the hash of your download:

   ```
   Get-FileHash .\<file> -Algorithm SHA256        # Windows, PowerShell
   shasum -a 256 <file>                           # macOS
   ```

2. **Build provenance.** With the [GitHub CLI](https://cli.github.com/) installed:

   ```
   gh attestation verify <file> --repo bcankara/scholar-citation-tracker
   ```

   This confirms that GitHub Actions of this repository built exactly that file.

### Updates

After the first installation, updates are installed from inside the app. About a minute after start and then once a day (switch: **Settings > Check for updates automatically**), the app looks for a newer release. When one exists, a bar shows "Version X is ready" with the release notes and an **Update now** button: it downloads the update with a progress indication, installs it and restarts the app. On Windows that is the installer; on macOS the app bundle is replaced in place. **Settings > About** shows the version, the GitHub link and the license, and has a **Check for updates** button that always reports its result (background check failures are silent). Updates are signed with the project's updater key, which is built into the app and checked before anything is installed, so this path does not show the SmartScreen or Gatekeeper warning again.

## Uninstalling

**Windows.** Use **Settings → Apps → Installed apps → Scholar Citation Tracker → Uninstall**, or right-click the Start menu entry and choose **Uninstall**; **Settings → About → Uninstall…** inside the app opens that Windows page. The uninstaller offers to delete the app data. The citation database stays in `Documents\ScholarTakip` unless you delete that folder (or you changed the data folder in Settings), and the settings are in `%APPDATA%\io.github.bcankara.scholarcitationtracker`.

**macOS.** Turn **Run at login** off in Settings first, so the app removes its Launch Agent, then quit from the menu-bar icon and drag the app from **Applications** to the Trash. Nothing else is installed outside your home folder. What stays behind: the settings in `~/Library/Application Support/io.github.bcankara.scholarcitationtracker` and the citation database in `~/Documents/ScholarTakip` (or the data folder you chose in Settings); delete both if you want it gone completely.

## Build from source

Prerequisites on **Windows**:

- Rust (stable, MSVC toolchain)
- Visual Studio C++ Build Tools (Desktop development with C++)
- Microsoft WebView2 runtime
- Tauri CLI: `cargo install tauri-cli --version "^2"`

Prerequisites on **macOS** (11 or newer):

- Rust (stable); for a universal build also `rustup target add aarch64-apple-darwin x86_64-apple-darwin`
- Xcode Command Line Tools: `xcode-select --install`
- Tauri CLI: `cargo install tauri-cli --version "^2"`

Node.js and npm are not needed; the UI is plain HTML/CSS/JS in `ui/`.

### Testing without touching your installed app

Build and run a **dev identity** so that nothing collides with a real installation (settings, WebView data, single-instance lock and autostart entry differ; the Credential Manager entries differ when `SCT_KEYRING_SERVICE` is set, see below):

```
cargo tauri build --debug --no-bundle --config src-tauri/tauri.dev.conf.json
```

`src-tauri/tauri.dev.conf.json` sets the identifier `io.github.bcankara.scholarcitationtracker.dev` and the product name "Scholar Citation Tracker Dev". Its settings live in `%APPDATA%\io.github.bcankara.scholarcitationtracker.dev` (`~/Library/Application Support/io.github.bcankara.scholarcitationtracker.dev` on macOS), its WebView data in `%LOCALAPPDATA%\io.github.bcankara.scholarcitationtracker.dev` on Windows, the single-instance mutex is `<identifier>-sim` and the autostart value is named after the product name. Put an explicit temporary `data_dir` and `"autostart": false, "auto_update": false` in its `settings.json`, and delete the `.dev` folders when you are done. Debug builds also honour `SCT_KEYRING_SERVICE` (use a separate credential store service name) and `SCT_TEST_PDF_PATH` (write the report PDF there instead of asking where to save); release builds ignore both.

`node tools/gen_licenses.js` regenerates `ui/licenses/` (the Rust crates compiled into the app with their licences, for the Credits page) from `src-tauri/Cargo.lock`; run it after every dependency change. `node tools/gen_licenses.js --check` fails when the files are out of date (the release workflow runs it).

`node tools/check_i18n.js` checks that the Turkish and English strings match, that every string the UI uses exists, and that no abbreviated period labels (30G, 12A, 3Y) are left.

`node tools/check_ojp_contrast.js` recomputes the contrast of the OJP badge (number against fill, 10 steps, light and dark theme, report) from the CSS and fails below 4.5:1.

```
cd src-tauri
cargo test
cargo tauri build                                  # Windows, or the host Mac only
cargo tauri build --target universal-apple-darwin  # macOS, both architectures
```

Because updater artifacts are enabled, `cargo tauri build` needs a signing key: set `TAURI_SIGNING_PRIVATE_KEY_PATH` (path to a key file made with `cargo tauri signer generate`) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; the matching public key goes into `plugins.updater.pubkey` in `tauri.conf.json`. The installer and its `.sig` are written to `src-tauri\target\release\bundle\nsis\`; on macOS the `.app`, `.dmg` and the signed `.app.tar.gz` land in `src-tauri/target/universal-apple-darwin/release/bundle/`.

Platform settings that only apply to one target live in their own config file, which the Tauri CLI merges automatically: `src-tauri/tauri.macos.conf.json` holds the macOS bundle targets, the minimum system version and `macOSPrivateApi` (needed for the translucent window).

The interface writes its icons as private-use code points of "Segoe Fluent Icons", a font that ships with Windows and is installed nowhere else. `ui/icons.js` measures whether that font can draw; where it cannot, each code point is swapped for the same shape as inline SVG, which inherits the colour and the size of the element it replaces, so the existing size rules keep working. Only an element that holds the code point and nothing else is swapped: a button that wraps an icon beside a label reads as that one character too while the label is still empty, and overwriting it would take the label with it. On Windows the file does nothing and the system font is drawn as before. A code point without a shape is left alone and named once in the console, so an icon added later shows as a missing shape rather than a silent blank.

To check that the live page can still be parsed (one request), run `cargo run -- --once <profile id or profile URL>` from `src-tauri`; a debug build prints the result to the console.

Tests use a small hand-written profile page that is part of the test code. If you place a saved copy of a real profile page at `src-tauri/tests/fixtures/profile.html`, a few extra consistency checks run against it; without that file they are skipped. The `fixtures` folder is git-ignored.

## Releasing (maintainers)

Pushing a tag `vX.Y.Z` that matches the version in `src-tauri/tauri.conf.json` starts `.github/workflows/release.yml` on GitHub Actions. It runs the tests and builds on both platforms in parallel: the NSIS installer on `windows-latest` and the universal `.dmg` on `macos-latest`, both signed with the updater key from the `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets. It creates a **draft** release containing both downloads, their `.sig` files, `SHA256SUMS-windows.txt`, `SHA256SUMS-macos.txt` and a build provenance attestation per platform. A third job then writes `latest.json` once from both builds, so the updater feed names `windows-x86_64`, `darwin-aarch64` and `darwin-x86_64` in one file; it fails the run if any of the three is missing. Review the draft and publish it. The in-app updater reads `https://github.com/bcankara/scholar-citation-tracker/releases/latest/download/latest.json`, which only exists once a release is **published** (a draft is invisible to it). A manual run of the workflow (*Run workflow*) builds the same files as workflow artifacts and creates no release.

## Disclaimer

This is an unofficial, independent project. It is not affiliated with, endorsed by or sponsored by Google. It reads the publicly visible profile page at a low request rate (by default once every 3 hours) and does not use any private API. Google Scholar has no official API, and its pages or terms can change, which may break the app at any time. You are responsible for making sure your use complies with Google's terms of service. The app is intended for personal use, to follow your own profile; do not use it for bulk or high-frequency scraping.

## Licences and credits

The same list is in the app (Settings > About > Licences and credits), where every licence text can be opened.

### Data sources

- **OpenAlex** (https://openalex.org): metadata of the citing works (journal, authors, institutions, countries, fields). OpenAlex data is released under CC0.
- **Opindx** (OJP scores), see below.
- **Natural Earth** (world map), see below.

### Libraries, icons and fonts

The application icon is based on Fluent UI System Icons by Microsoft (MIT License).

Charts are drawn with [Apache ECharts](https://echarts.apache.org/) 5.5.1 (Apache License 2.0), bundled in `ui/vendor/` with its license and notice (`echarts.min.js`, SHA-256 `e84270bd0cd5bdf60fefc26d00c2a391cb2e81f4d26a7a9ee16185a54773a3cf`, from the npm registry package `echarts@5.5.1`); it works offline and nothing is loaded from a CDN.

World map: [Natural Earth](https://www.naturalearthdata.com/) 1:110m country boundaries (public domain), tag `v5.1.2` of `nvkelso/natural-earth-vector`, reduced to `ui/vendor/world.geo.json` (SHA-256 `5639a2568f0f14a602ba21da1310e9aebcc3dc6afb21417e4c96af7d64931e31`; source file `6866c877d39cba9c357620878839b336d569f8c662d3cfab4cb1dbe2d39c977f`); notice in `ui/vendor/LICENSE-natural-earth.txt`.

OJP data: Weitzel, U. (2026). *Opindx: Open Journal Factor, Network Factor and Article Network Score* (dataset version 2026-09-v5) [Data set]. Zenodo. https://doi.org/10.5281/zenodo.23599869, used under the [Creative Commons Attribution 4.0 International licence (CC BY 4.0)](https://creativecommons.org/licenses/by/4.0/). Changes made: from the file for citation year 2024 only the OpenAlex-universe Article Network Score (Raw) column and the journal identification columns were taken (the publication count and the OpenAlex-universe flag were also read, for filtering; no column derived from the Norwegian Register was used); the scores were converted to within-field percentile ranks (OJP, 0-100); journals with fewer than 20 counted articles, without a score or field, or in fields with fewer than 20 ranked journals were left without a score. OJP is not an official Opindx metric; the author of the dataset has not reviewed or endorsed this derived work. For the columns used here, the numerical input of Opindx is OpenAlex data (CC0, https://openalex.org). The bundled file is `src-tauri/data/ojp/ojp-2024.csv.gz` (SHA-256 `034462a329917013c8f250e30762dc5b99ce88664bcd790b2193f36414c04e1b`; source file `scores_2024.csv` SHA-256 `788de513f9d69f0418aa5798e755314b1fcba2312c2f419beac1531d8f0a1ad4`); attribution text in `src-tauri/data/ojp/NOTICE.txt`.

Licences of the Rust libraries compiled into the app: `ui/licenses/rust-crates.json` and the texts in `ui/licenses/rust-license-texts.json` (generated by `tools/gen_licenses.js`; most are MIT or Apache-2.0, a few MPL-2.0 and Unicode-3.0). The local database is SQLite (public domain, bundled through the `rusqlite` crate). The app bundles no fonts; it uses Segoe UI and Segoe Fluent Icons as installed with Windows. The app itself is released under the [MIT licence](LICENSE).

## License

[MIT](LICENSE)
