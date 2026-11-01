# Verification of the Delhi battle royale notes

Checked 1 November 2026. I read all seven notes, picked the 15 claims the final report leans on most, and tried to knock each one down with new searches and sources the notes did not use. I then compared the notes' ideas with `game_code_check.md` and re-read the key code lines. Nothing was built or run.

**Verdicts**

- **Confirmed:** a fresh source backs the main point and nothing found contradicts it. Small fixes are listed.
- **Weak:** the main point may be right, but the evidence is thin, old or single-source, or the wording goes further than the evidence.
- **Wrong:** fresh evidence contradicts it.

**Source marks:** (opened) means I read the page. (snippet) means I saw only a search extract, usually because the page blocked me. Repo paths use the short forms from `game_code_check.md`.

## Result

Eight claims are confirmed, seven are weak and none is flatly wrong. Some confirmed claims still need a small fix.

What should change in the final report:

1. **Real places of worship are already in the rotation maps.** Gurdwara Sis Ganj Sahib stands 67 m from the centre of the Chandni Chowk map. The notes' rule against fighting at such sites clashes with what is shipping today. The owner needs to decide this first (see the next section).
2. **Say "no motor vehicles", not "no vehicles", for PUBG's smallest maps.** Small maps do not always drop vehicles: Livik and Rebirth Island keep them.
3. **Do not tune vehicles from the Warzone health numbers.** They come from one site with no stated method.
4. **The US legal comfort is smaller than the notes imply.** This makes the made-up brands advice stronger, not weaker.
5. **Do not ship "Bo kata!" as a Delhi cry yet.** The sources for that exact form are from Lahore.
6. **Keep Palika Bazaar as a place, but drop its numbers.**

| # | Claim | From | Verdict | What the report should say |
|---|---|---|---|---|
| 1 | Real e-rickshaw (25 km/h) and tractor (about 30 km/h) barely match a Panga sprint | vehicles.md | Confirmed | Keep. Give the tractor as 28.5 to 31.3 km/h by variant |
| 2 | Speed varies little between vehicles, toughness varies a lot | vehicles.md | Weak | True of Warzone speeds only. PUBG and Free Fire have big speed gaps |
| 3 | PUBG's Haven and Karakin give players no drivable vehicles at all | vehicles.md | Weak | No motor vehicles. A folding bicycle can spawn |
| 4 | Top rifles are gated: AWM is crate-only, Kraber has 12 shots | guns.md | Confirmed | Keep. Drop "Mythic bins" |
| 5 | Warzone ranked caps sniper damage at 299 | guns.md | Weak | One-shot protection still exists. 299 is a 2024 figure |
| 6 | PUBG moved Blue Zone damage from distance to time in June 2026 | battle_royale_design.md | Confirmed | Keep |
| 7 | The US position favours games (AM General v Activision) | guns.md, vehicles.md | Weak | Narrowed in 2023. Realism inside a game may still be safe, a brand in a title is not |
| 8 | Indian law protects reputed marks on unrelated goods, and Tata v Greenpeace gives a commercial game little cover | vehicles.md | Confirmed | Keep. The Bench ruling is from 6 March 2024, not 2025. Say "a real claim", not "strong" |
| 9 | India's game rating standard is voluntary IS 19690:2026, and IGRS is Indonesia's | guns.md | Confirmed | Keep, and state the date of the last check |
| 10 | Palika Bazaar has 380 shops, seven gates and 15,000 people inside at any time | hiding.md | Weak | The location holds. Drop the numbers |
| 11 | Hanuman Mandir, Jantar Mantar and Agrasen ki Baoli are off the Connaught Place map | hiding.md | Confirmed | Keep. Now backed by coordinates |
| 12 | Cabs and autos stop at the edge of old Chandni Chowk | hiding.md | Confirmed | Keep, and add the daytime motor vehicle ban |
| 13 | The .303 rifle is spoken of as "thri-not-thri" | guns.md | Confirmed | Keep. "Teen-Pandrah" stays unconfirmed |
| 14 | "Bo kata" is a local Delhi kite cry | delhi_culture.md | Weak | Delhi sources give "Wooh kata" and "Ayi Bo". A native picks |
| 15 | Chhapri comes from the Chapparband caste | delhi_culture.md, indian_games.md | Weak | Origin unproven. Keep the ban, because the casteist reading is public |

## Biggest finding: real places of worship on the rotation maps

`indian_games.md` says not to make real temples, gurdwaras, mosques or dargahs into fight arenas or loot spots, and `delhi_culture.md` says no place of worship should be a fight zone. `hiding.md` asked for a check of smaller shrines around Connaught Place only. Nobody checked the other maps. The code check says all four Delhi maps come from OpenStreetMap and all four are in the rotation.

I queried OpenStreetMap for each 500 m play square on 1 November 2026 and matched the results to the footprints in `assets/data/maps/`.

| Map | Site | Game position (x, z) | In the game today |
|---|---|---|---|
| Chandni Chowk | Gurdwara Sis Ganj Sahib, where Guru Tegh Bahadur was executed | 61, -28 | Solid unnamed block, 17.8 m tall, 67 m from map centre |
| Chandni Chowk | Shahi Sunehri Masjid | 10, -40 | Named in the map file as kind `mosque`, with a red sandstone material (`map/delhi.rs:341`) |
| Chandni Chowk | Jain Sweitamna Temple | -86, 66 | Shown as a HUD place name |
| Chandni Chowk | Central Baptist Church | 199, -78 | Solid unnamed block |
| Chandni Chowk | Several unnamed Hindu temple entries | near -126, -136 and -209, -72 | No matching footprint, other buildings within about 20 m |
| Hauz Khas | Hauz Khas Medresse Mosque, Feroz Shah Tughlaq's tomb | 23, 3 and -25, 109 | Mosque is a named block. Madrasa and tomb names are HUD labels |
| Hauz Khas | New City Delhi church, an unnamed Hindu temple entry | 81, -14 and 66, 31 | Church sits on a plain retail block, and its name is a HUD label. The temple has no footprint |
| Connaught Place | Moti Masjid, B Block | -127, -149 | Plain 11 m commercial block |
| India Gate | No place of worship found. India Gate is a war memorial | 116, -4 | One of the two built landmarks |

- Moti Masjid's current state is unclear. A post from about 2017 shows a working mosque, and a 2018 post says the old B Block building was demolished to make more room for prayer.
- Sis Ganj is the sharpest risk. Hitman 2 had to change a shipped game after Sikh youth objected to a temple level that resembled the Golden Temple.
- This affects the code check's call that Chandni Chowk is "the best driving map".
- I did not check whether chests or floor loot land next to these sites.
- **Owner decision needed.** Options include no loot or HUD names near these sites, no landing on them, review by local Sikh and Muslim players, or replacing the map. The notes do not settle this.

Sources: [OpenStreetMap Overpass query, run 1 Nov 2026](https://overpass-api.de/); [Wikipedia, Gurdwara Sis Ganj Sahib](https://en.wikipedia.org/wiki/Gurudwara_Sis_Ganj_Sahib) (opened); [Hinduism Today](https://www.hinduismtoday.com/hpi/2002/11/21/2002-11-21-sikh-kids-force-changes-in-video-game/) (opened, the note's own source, second source not checked); [Rana Safvi](https://ranasafvi.com/moti-masjid/amp/) (opened); [Sarson Ke Khet, 2018](https://sarsonkekhet.in/2018/03/30/a-connaught-place-mosque-is-no-more/) (opened).

## The 15 claims

### 1. Real e-rickshaw and tractor speeds barely match a Panga sprint: Confirmed

- **Claim:** a legal e-rickshaw is capped at 25 km/h (6.9 m/s) and a Mahindra 575 DI tractor tops out at 29.5 km/h (8.2 m/s), against a 7.4 m/s sprint.
- **Tried:** looked for a different legal limit and for higher tractor figures.
- **Found:**
  - Rule 2(cb) of the Central Motor Vehicles Rules caps e-rickshaws at 2,000 W and 25 km/h. This closes the gap in `vehicles.md`, which had only the 2026 Delhi draft confirmed.
  - The 2026 Delhi draft repeats those limits and is not in force.
  - The base 575 DI is listed at 29.5 km/h on several sites. Variants run 28.5 to 31.3 km/h. I found no Mahindra brochure.
  - `SPRINT_SPEED` is 7.4 in `sim/movement.rs`.

Sources: [CMVR extract, Commercial Law Publishers](https://commerciallawpublishers.com/cdn/shop/files/sample_chapter_cmv_13c382c1-7c7f-47b2-8959-ac926f621bdd.pdf) (snippet); [Deccan Herald, 2014](https://www.deccanherald.com/archives/e-ricks-get-legal-status-2208488) (snippet); [Outlook, Oct 2026](https://www.outlookindia.com/national/delhi-e-rickshaw-rules-2026-four-passenger-limit-proposed-driving-licence-and-psv-badge-may-become-mandatory) (opened); [Tractor News](https://cdn.tractornews.in/tractors/mahindra-575-di) (snippet); [Merikheti XP Plus](https://www.merikheti.com/en/tractor/mahindra-575-di-xp-plus) (snippet); [Tractor For Everyone SP Plus](https://www.tractorforeveryone.com/used-tractors/mahindra-575-di-sp-plus-2wd-tractor-2019) (snippet).

### 2. Speed varies little between vehicles, toughness varies a lot: Weak

- **Claim:** Warzone 1 land vehicles run 15 to 19 m/s while health runs 900 to 3,600, and identity comes from seats, noise and exposure across all five games.
- **Tried:** looked for a second source for each Warzone figure, and tested the pattern against the other games.
- **Found:**
  - Every Warzone 1 figure traces to one RacingGames article from June 2021, which states no method. I found no second source for any health number.
  - The speed band holds up: JGOD's Warzone 2 tests give 15.3 to 19.8 m/s for land vehicles. That is a different game.
  - Other guides agree on the order only: cargo truck slowest and toughest, ATV and Rover exposed.
  - The pattern does not carry across games. `vehicles.md`'s own table has PUBG at 27.8 to 42.2 m/s and Free Fire at about 100 to 160 km/h.
  - `vehicles.md` says to avoid big speed gaps, but its own proposal compresses main vehicles to 9 to 17 m/s, nearly a 2 times spread.

Sources: [RacingGames](https://racinggames.gg/misc/cod-call-of-duty-warzone-vehicle-vehicles-verdansk-chopper-helicopter-suv-bertha-atv-quad-tac-rover-health-speed) (opened); [ONE Esports on JGOD](https://www.oneesports.gg/call-of-duty/fastest-vehicles-warzone-2/) (opened); [TheGamer](https://www.thegamer.com/all-vehicles-in-cod-warzone-ranked/) (opened); [Gamer Guides](https://gamerguides.com/call-of-duty-warzone/guide/basics/how-to-play/vehicles) (opened).

### 3. Haven and Karakin give players no drivable vehicles at all: Weak as worded

- **Claim:** PUBG's two smallest PC maps have no drivable vehicles, and Connaught Place is smaller than both.
- **Tried:** looked for any vehicle on either map, in game data, patch notes and guides.
- **Found:**
  - No motor vehicle spawns on either map. A fan database of game files, updated 28 October 2026, reports zero vehicle spots on both. The PUBG wiki agrees.
  - Krafton's Dirt Bike was made for every map "with the exception of Karakin".
  - The folding Mountain Bike is the exception. The 15.1 patch notes say "All maps in normal matches". The fan database lists a `bicycle` loot code on both maps, though it does not expand the code. A 2023 guide says Karakin has no vehicles "except for the bicycle".
  - Small maps do not always drop vehicles. Livik is the same size as Karakin and has them. Rebirth Island's vehicles were back on 13 April 2022, about two days after being disabled, and a Blizzard post from about January 2026 still lists them. This answers a gap in `vehicles.md`.

Sources: [pubg.gs Karakin tables](https://pubg.gs/maps/karakin/tables/) (opened); [pubg.gs Haven tables](https://pubg.gs/maps/haven/tables/) (opened); [PUBG wiki, Karakin](https://pubg.wiki.gg/wiki/Karakin) (opened); [PUBG wiki, Haven](https://pubg.wiki.gg/wiki/Haven) (opened); [Krafton 9.2](https://press.krafton.com/PUBG-Update-92-available-now-on-PC) (opened); [RealSport101, 15.1 notes](https://realsport101.com/article/pubg-151-patch-notes-release-date-mountain-bike-christmas-collab) (opened); [GamersDecide](https://gamersdecide.com/articles/pubg-best-karakin-drop-locations) (opened); [Charlie Intel, 13 Apr 2022](https://charlieintel.com/warzone-april-13-patch-notes/171248) (opened); [Blizzard, Rebirth Island](https://news.blizzard.com/en-gb/article/24250352/warzone) (opened).

### 4. Top rifles are gated by drops and ammo: Confirmed, with one stale detail

- **Claim:** AWM is air-drop only, 945 m/s, 5 rounds, 262.5 head damage falling to 118.1 at a level 3 helmet. Kraber does 150 body and 210 head, has 12 shots, 749 m/s, and comes only from care packages or Mythic bins.
- **Tried:** looked for 2025 and 2026 patches that changed either gun or moved it into normal loot.
- **Found:**
  - A site on PUBG 43.2 data gives 105 base damage, 262.5 to the head and "spawns only in care packages". Its changelog to 43.1 shows no AWM damage change.
  - One conflict: the pubg.gs fan database shows 69 base damage and "Spawns in world: yes". Two other sources and the changelog disagree with it, so I treat this as a data reading error, not a proven change. If it were right, the AWM would not one-shot a level 3 helmet.
  - The 118.1 figure still rests on the wiki at patch 22.2.8.1.
  - Kraber's numbers are unchanged in the wiki, and the Marked and Marked Midseason notes do not mention it.
  - **Stale:** the August 2026 Marked notes list "Removed Mythic Bins" without naming a mode. The October 2026 Cipher update calls Strongholds "the evolution of Care Packages". Write "crate weapon" and do not name the bins.
  - Only one weak site places the Kraber in the Season 30 care package.

Sources: [PUBG Statistics, AWM](https://pubgstatistics.com/weapons/awm) (opened); [PUBG Statistics changelog](https://pubgstatistics.com/weapons/patch-notes) (opened); [pubg.gs AWM](https://pubg.gs/weapons/awm/) (opened); [PUBG wiki, AWM](https://pubg.wiki.gg/wiki/AWM) (opened); [Apex wiki, Kraber](https://apexlegends.wiki.gg/wiki/Kraber_.50-Cal_Sniper) (opened); [EA, Marked notes](https://www.ea.com/games/apex-legends/apex-legends/news/marked-patch-notes) (opened); [EA, Cipher updates](https://www.ea.com/games/apex-legends/apex-legends/news/cipher-game-updates) (opened); [Everything Edinburgh](https://everythingedinburgh.com/games/gaming/apex-legends-best-guns/) (opened, weak).

### 5. Warzone ranked caps sniper damage at 299: Weak for the number

- **Claim:** in Resurgence Ranked, snipers cannot down in one shot and damage is capped at 299, which reads as one point under full health.
- **Tried:** looked for the current rule in 2026 notes.
- **Found:**
  - The 299 figure traces to early-2024 Season 2 coverage. A later Z League piece repeats it, and that page is now gone.
  - The idea is still alive. September 2026 Season 06 coverage says Raven extended "the one-shot protection already used on snipers" to new Ranked items. No current source gives a number.
  - The official Season 06 page timed out twice, so I read it only as a search extract.
  - Charlie Intel puts a fully plated player at 300 health, which fits the note's reading. That piece is from Warzone 2 and also gives a normal 249 sniper cap, so the numbers have moved across versions.

Sources: [GG Clan, Season 06](https://ggclan.com/news/warzone-season-06-ranked-sr-weapon-changes) (opened); [Charlie Intel](https://www.charlieintel.com/call-of-duty-warzone/warzone-2-devs-remove-one-shot-sniper-rifles-with-secret-nerf-231767/) (opened); [Call of Duty, Season 06 notes](https://www.callofduty.com/patchnotes/2026/09/call-of-duty-bo7-warzone-season-06-patch-notes) (snippet); [Sportskeeda](https://sportskeeda.com/call-of-duty-game/all-rules-regulations-warzone-ranked-resurgence) (snippet); [Z League](https://www.zleague.gg/theportal/warzone-snipers-can-they-still-one-shot-in-ranked-matches/) (snippet, page now 404).

### 6. PUBG moved Blue Zone damage from distance to time: Confirmed

- **Claim:** the June 2026 rework scales damage with time in the zone, and Ranked drops from 32m 50s to 30m 10s. This is the top item on the copy list in `battle_royale_design.md`.
- **Tried:** looked for a source saying this did not ship or works differently.
- **Found:**
  - The 16 June 2026 Dev Letter says damage "will now scale based on the amount of time spent inside the Blue Zone", and that it previously "scaled based on a player's distance from the safe zone".
  - Console notes give Ranked 32m 50s to 30m 10s and Normal unchanged at 28m 50s, which closes a gap in the note.
  - **Limit:** no source says how time is counted or whether it resets on leaving. Panga's "one float" design is our own choice, not a copy of PUBG's rule.

Sources: [PUBG Dev Letter](https://pubg.com/news/10280) (opened); [Mp1st](https://mp1st.com/title-updates-and-patches/pubg-update-1-017-drops-to-bring-42-1-on-consoles) (opened); [Inven Global](https://www.invenglobal.com/articles/22861/krafton-to-revamp-playerunknowns-battlegrounds-blue-zone) (opened); [patched.gg](https://patched.gg/games/pubg-battlegrounds/patch-notes-update-421) (opened).

### 7. The US position favours games: Weak

- **Claim:** AM General lost its Humvee case against Activision in 2020 under the Rogers test, so US law favours games. Both notes say later treatment was not checked.
- **Tried:** checked what happened to the Rogers test after 2020.
- **Found:**
  - In Jack Daniel's v VIP (June 2023) the US Supreme Court held unanimously that Rogers does not apply when a mark is used to name the user's own goods.
  - On remand in Activision v Warzone.com, the court held Rogers was not the right framework for "Warzone" as a game title.
  - One firm's view (October 2024) is that realism cases remain viable, because the marks are small parts of a larger work. I found no post-2023 court ruling on realistic in-game use.
  - I could not re-read AM General itself: PC Gamer showed only its headline and Finnegan returned 403. The 2020 result is not in doubt, but its reach is.
  - For Panga, this adds weight to made-up brands. A real brand used in a game's title is unlikely to get Rogers shelter, and the same risk probably extends to marketing.

Sources: [Robins Kaplan, Daily Journal, May 2024](https://robinskaplan.com/-/media/pdfs/publications/warzone-clash-when-does-a-video-game-title-cross-the-link-into-trademark-infringement) (opened); [Fennemore via Legal 500](https://www.legal500.com/intelligence/united-states/intellectual-property/the-use-of-trademarks-in-video-games-does-realism-serve-as-a-defense-to-infringement) (opened); [Wikipedia, Rogers v. Grimaldi](https://en.wikipedia.org/wiki/Rogers_v._Grimaldi) (opened); [Wikipedia, Jack Daniel's v. VIP](https://en.wikipedia.org/wiki/Jack_Daniel%27s_Properties,_Inc._v._VIP_Products_LLC) (snippet).

### 8. Indian law and the Tata precedent: Confirmed, with a date fix

- **Claim:** section 29(4) of the Trade Marks Act covers dissimilar goods, a 2025 Delhi Division Bench made "reputation" a lower bar than "well-known", and Tata v Greenpeace helps a commercial game little.
- **Tried:** looked for the judgment itself, for contrary rulings, and for any Indian case on a car or bike brand in a game.
- **Found:**
  - **Date fix:** the ruling is Crompton Greaves v V-Guard, 6 March 2024, 2024:DHC:1852-DB, Justices Bakhru and Mahajan. The law firm blog the note cites is from May 2025. Company filings report a dismissed appeal to the Supreme Court and a final decree in January 2025.
  - The "lower bar" wording comes from that blog. I could not read the judgment.
  - All section 29(4) conditions must be met together, and courts have refused relief. In Kamdhenu Ispat v Kamdhenu Pickles (Delhi, 2010) a steel brand could not stop a pickle maker, because its reputation did not reach that trade. So "a real claim" is safer wording than "strong".
  - Tata facts hold: Justice Ravindra Bhat, 28 January 2011, Rs 10 crore sought, interim injunction refused. The court noted Greenpeace was not in competing profit-making activity, so a commercial game gets little from it. It was an interim order only.
  - One more search found no Indian case on a car or bike brand in a game.

Sources: [Cyril Amarchand](https://corporate.cyrilamarchandblogs.com/2025/05/weather-crompton-pebble-and-pebble-are-similar-or-identical/) (opened); [Legal Service India](https://www.legalserviceindia.com/legal/article-15673-section-29-4-trademarks-act-1999-and-prima-facie-satisfaction-of-goodwill.html) (opened); [Crompton filing, Jan 2025](https://nsearchives.nseindia.com/corporate/Crompton123_31012025190807_IntimationforOrderspassedhighcourt.pdf) (snippet); [Nishith Desai on Kamdhenu](https://nishithdesai.com/fileadmin/user_upload/pdfs/Research%20Articles/Court%20refuses%20to%20prevent%20use%20of%20identical%20mark%20for%20dissimilar%20goods.pdf) (opened); [Columbia Global Freedom of Expression](https://globalfreedomofexpression.columbia.edu/cases/tata-sons-ltd-v-greenpeace-international/) (opened); [Governance Now](https://www.governancenow.com/gov-next/green-gov/tata-vs-turtle-game-not-be-banned-delhi-hc) (snippet).

### 9. India's rating standard is voluntary IS 19690:2026: Confirmed

- **Claim:** BIS notified IS 19690:2026 in March 2026, it is voluntary and self-assessed, and "IGRS" is Indonesia's system, not India's.
- **Tried:** looked for an Indian system called IGRS and for any move to make the standard mandatory.
- **Found:**
  - Several sources agree. The date is 10 March in most and 11 March in one.
  - The stray "mandatory" sentence in Outlook is there, and the same article twice says voluntary.
  - No Indian "IGRS" turned up.
  - The Online Gaming Rules were notified on 22 April 2026 and took effect on 1 May, which also backs `guns.md`.
  - **Limit:** my newest source on voluntary status is from April 2026, and I could not read the standard's text.

Sources: [Outlook Respawn](https://respawn.outlookindia.com/gaming/gaming-originals/indias-first-game-rating-standard-real-need-or-overreach) (opened); [Storyboard18](https://www.storyboard18.com/digital/bis-issues-framework-for-age-rating-of-video-and-digital-games-in-india-ws-e-94408.htm) (snippet); [Digital Policy Alert](https://digitalpolicyalert.org/event/39127-bureau-of-indian-standards-adopted-standard-on-age-rating-and-content-descriptors-for-video-and-digital-games) (snippet); [SCC Online](https://www.scconline.com/blog/post/2026/04/25/promotion-and-regulation-of-online-gaming-rules-2026-key-highlights/) (snippet); [Business Today](https://www.businesstoday.in/technology/story/online-gaming-rules-notified-govt-opts-for-minimal-compliance-selective-oversight-526973-2026-04-22) (snippet).

### 10. Palika Bazaar: 380 shops, seven gates, 15,000 people at any time: Weak

- **Claim:** as above, from Wikipedia, with "about 425 shops" noted as an alternative. The notes turn the gate list into game rules.
- **Tried:** looked for an official shop count, and checked the crowd figure and gate list.
- **Found:**
  - The location holds. Wikipedia's coordinates land at x -105, z 163, about 39 m from the map's label and 228 m from the park centre.
  - The crowd figure conflicts. The Patriot says about 15,000 visitors **daily** in the 2000s and 2010s, not 15,000 inside at once. For 380 to 425 shops, the daily figure is far more believable.
  - The shop count stays 380 or about 425. No official NDMC count turned up.
  - Wikipedia itself flags its gate section as unsourced, and the magicpin travel blog says eight gates, though it also says about 380 shops.
  - `hiding.md`'s table also says "six gates" while its own source lists seven with one closed.
- **Use:** treat the gate types as inspiration, not fact. No floor plan exists in anything I read.

Sources: [Wikipedia](https://en.wikipedia.org/wiki/Palika_Bazaar) (opened); [The Patriot](https://thepatriot.in/reports/palika-bazaar-from-a-shoppers-paradise-to-a-relic-of-nostalgia-67942/amp) (opened); [magicpin](https://magicpin.in/blog/india/delhi/Travel/palika-bazaar-delhi) (opened, weak); [NDMC council paper](https://www.ndmc.gov.in/council/Resolutions%202007/CIVIL/Civil%2020.08.08/ITEM%20NO.%2018%20(A-46).doc) (snippet).

### 11. Hanuman Mandir, Jantar Mantar and Agrasen ki Baoli are off the map: Confirmed

- **Claim:** none of the three sits on the Connaught Place map. `hiding.md` could only partly compute this.
- **Tried:** converted sourced coordinates using the same projection as `tools/osm/delhi.py`, including the 40 m south shift. The play square is 250 m each way.

| Site | Game position (x, z) | Past the play edge |
|---|---|---|
| Hanuman Mandir (OpenStreetMap) | -454, 235 | 204 m west |
| Jantar Mantar | -320, 597 | 347 m south |
| Agrasen ki Baoli | 521, 707 | 457 m south |
| Gurdwara Bangla Sahib | -1,052, 641 | 802 m west |

- The Hanuman Mandir is also outside the 300 m zone kept for the skyline, so it is not drawn.
- "About 250 m southwest of CP" is measured from CP's outer edge. From the park centre it is about 531 m.
- `hiding.md`'s 330 m for Jantar Mantar is close to my 347 m.
- **Caution:** this does not clear the map. See Moti Masjid above.

Sources: [Wikipedia, Hanuman Temple](https://en.wikipedia.org/wiki/Hanuman_Temple,_Connaught_Place) (opened); [Wikipedia, Jantar Mantar](https://en.wikipedia.org/wiki/Jantar_Mantar,_New_Delhi) (opened); [Wikipedia, Agrasen ki Baoli](https://en.wikipedia.org/wiki/Agrasen_ki_Baoli) (opened); [Wikipedia, Bangla Sahib](https://en.wikipedia.org/wiki/Gurudwara_Bangla_Sahib) (opened); OpenStreetMap Overpass query, 1 Nov 2026.

### 12. Cabs and autos stop at the edge of old Chandni Chowk: Confirmed

- **Claim:** the lanes are too narrow, so vehicles stop at the edge.
- **Tried:** looked for evidence that motor vehicles do run there.
- **Found:**
  - The real position is stronger. A June 2021 gazette notification bars motor vehicles from the main road, Red Fort to Fatehpuri Mosque, 9 am to 9 pm, with emergency exceptions.
  - A 2023 report says the ban was openly flouted.
  - Sources disagree on whether e-rickshaws are allowed on the main road. Zingbus says they run in parts of the market.
  - This conflicts with the code check's "best driving map". Its "mostly 7 m streets" is a default width by road class (`tools/osm/delhi.py:43-47`), not a survey.

Sources: [India TV, June 2021](https://www.indiatvnews.com/news/india/delhi-chandni-chowk-road-notified-as-non-motorised-zone-712405) (opened); [Zingbus](https://www.zingbus.com/blog/chandni-chowk-market-timings-shopping-guide/) (opened); [Hindustan Times via Magzter](https://magzter.com/fr/stories/newspaper/Hindustan-Times/MOTOR-VEHICLE-BAN-OPENLY-FLOUTED-IN-CHANDNI-CHOWK) (snippet); [Housing.com](https://housing.com/news/chandni-chowk-become-first-motorised-vehicle-free-locality-national-capital) (snippet); [Lokmat Times](https://www.lokmattimes.com/national/back-to-its-former-glory-delhis-chandni-chowk-will-be-shoppers-paradise/amp) (snippet).

### 13. The .303 is spoken of as "thri-not-thri": Confirmed

- **Claim:** this is the proposed main sniper name. `guns.md` had it only from general knowledge.
- **Tried:** searched the Hindi press for the spoken form, and rechecked the Delhi Police retirement story.
- **Found:**
  - Patrika's headline reads "3 नॉट 3 राइफल". A YourStory Hindi piece appears to write "थ्री नॉट थ्री". PTI's English copy uses "three-nought-three".
  - Delhi Police's plan to dismantle 7,000 rifles holds across PTI, Deccan Herald and ThePrint.
  - "Teen-Pandrah" for the .315 did not turn up. Sources say ".315 bore".
  - A 2022 report describes seized Delhi-NCR country-made pistols as .315 bore, so that number may point at street kattas more than a rifle. A Delhi native should check this.

Sources: [Patrika](https://www.patrika.com/varanasi-news/303-rifle-retire-in-up-police-on-republic-day-2020-5696009/) (opened); [YourStory Hindi](https://yourstory.com/hindi/this-special-rifle-say-goodbye-to-uttar-pradesh-police) (snippet); [Deccan Herald, PTI](https://www.deccanherald.com/india/delhi/delhi-police-to-bid-adieu-to-7000-british-era-303-rifles-2776240) (snippet); [ThePrint, PTI](https://theprint.in/india/delhi-police-to-bid-adieu-to-7000-british-era-303-rifles/1849951) (snippet); [Lokmat Times](https://www.lokmattimes.com/national/inter-state-illegal-arms-syndicate-busted-three-nabbed/amp) (snippet).

### 14. "Bo kata" is a local Delhi kite cry: Weak

- **Claim:** "Bo kata" is the victory cry, and the idea is one of the best local fits for sniper kills. Its main source is a British Council page.
- **Tried:** searched English and Hindi for Delhi use of that exact form.
- **Found:**
  - That British Council page describes a film "Filmed entirely in Lahore, Pakistan". A February 2026 Dawn piece also places "Bo kata!" in Lahore.
  - Delhi sources give other forms: "Ayi Bo" on Old Delhi rooftops, "Wooh kata" on Delhi streets, and "kataa!" in a piece on the walled city.
  - Hindi press in Nagpur uses "वो काटा". No Delhi source I could find uses "Bo kata".
  - This does not prove Delhi people never say it. The cry itself is real and the design idea stands, but only the exact words are in doubt.
- **Use:** "Wo kata!" as working text, then ask a Delhi native to choose between that and "Ayi bo".

Sources: [British Council](https://filmsandfestivals.britishcouncil.org/projects/bo-kata) (opened); [Dawn, Feb 2026](https://www.dawn.com/news/1969527) (opened); [The National, 2022](https://thenationalnews.com/world/asia/2022/08/14/india-celebrates-75-years-of-independence-with-kite-festival) (opened); [Sunday Guardian](https://latest.sundayguardianlive.com/?p=6023) (opened); [Counterview, 2023](https://counterview.in/2023/08/independence-day-kite-flying-in-walled.html) (snippet); [Navbharat Live](https://navbharatlive.com/photos/kite-flying-was-a-big-hit-in-nagpur-kites-were-flown-amidst-noise-1100857.html) (snippet).

### 15. Chhapri comes from the Chapparband caste: Weak on origin, ban stands

- **Claim:** the word traces to the Chapparband caste, once listed under the Criminal Tribes Act. `delhi_culture.md` marks it RED.
- **Tried:** looked for a linguistic source, and for the community's own record.
- **Found:**
  - No primary source ties the word to the caste. The FLAME piece, by a student and a professor, cites one 2023 paper. Homegrown quotes a Reddit comment.
  - Wikipedia's Chhaparband page describes roofers in Karnataka and Maharashtra and mentions neither the slur nor a criminal listing.
  - A 2007 Bombay High Court extract calls "Chhaperband" a denotified tribe, which gives the Criminal Tribes point partial support.
  - One sociology piece says the casteist link is less widely known.
- **Use:** "widely read as casteist, exact origin unproven". Keep it RED. The cost of leaving it out is nil, and the casteist reading is already public.

Sources: [FLAME University](https://www.flame.edu.in/in-the-media/digital-exclusion-how-chapri-and-digital-aesthetics-reinforce-caste-hierarchies) (opened); [Homegrown](https://homegrown.co.in/amp/story/homegrown-voices/heres-why-using-the-word-chapri-isnt-as-harmless-as-you-think-it-might-be) (opened); [Wikipedia, Chhaparband](https://en.wikipedia.org/wiki/Chhaparband) (opened); [Doing Sociology](https://doingsociology.org/2024/06/12/chapri-fashion-a-disruption-of-cultural-domination-in-the-field-of-mens-fashion-in-india-sohom-roy/) (snippet); [Bombay High Court, 2007](https://www.latestlaws.com/judgements/bombay-high-court/2007/march/2007-latest-caselaw-227-bom) (snippet).

## Ideas that clash with game_code_check.md

I re-read the main code lines in the working tree: `sim/bots.rs:103,365,445`, `sim/combat.rs:129-132,609`, `game/hud.rs:430-459`, `game/street.rs`, `map/kit.rs`, `sim/loot.rs:135-137` and `shared/items.rs`. Other code facts below are taken from the code check. The largest clash, real places of worship, is covered above.

### The code stops the idea as written

| Idea | Note | What the code says | Change needed |
|---|---|---|---|
| Drivable vehicles "chosen from the kerb-parked props" | vehicles.md | Parked pieces are a static box merged into 32 m chunk meshes. A drivable one must be a separate object | Spawn separate vehicles at seeded kerb spots. The map fingerprint test may need updating |
| Parked auto as a hide-in, with one code path for many existing props | hiding.md | Same merge, so no prop can be entered | Each hide-in prop is a new object with a hidden state |
| Tractor or Bolero shoves props, handcart can be pushed, crates can be shot apart | vehicles.md, hiding.md | Kit pieces are static, the physics solver is off, and only doors have health | Drop for now, or build a few as separate objects |
| Parked bus or tempo as a walk-in hide | vehicles.md | No bus or tempo on any rotation map. NCR's bus is a solid block prop | New model and interior, or drop |
| Tractor kept off India Gate, against "natural on India Gate lawns" | vehicles.md vs code check | Direct contradiction | Owner decides. Keep it off until then, because the code check did not weigh the 26 January 2021 images |
| Squad barks and a truce or revive emote | delhi_culture.md | No teams, chat, ping or emotes. All against all | Rewrite as solo taunts, or wait for teams |
| Jugaad scopes as separate finds | guns.md | The fix is a scope field on each weapon. There is no attachment system, and battle_royale_design.md says not to add one | Fix each scope to a rifle variant |
| Breaking a shop window is loud and draws players | delhi_culture.md | Only doors break. `sim/` has no window code, and bots cannot hear | New breakable plus the hearing work |
| Kill replay called "Third Umpire" | guns.md | No replay code exists | New system, or reuse the name elsewhere |
| Tractor "climbs kerbs", rays for kerbs | vehicles.md | Delhi maps are one flat pad. Kerbs exist only on NCR | No effect on rotation maps. Drop |

### Costs more than the note says

| Idea | Note | What the code says | Effect |
|---|---|---|---|
| DTC bus on rails as the "cheapest first step" | vehicles.md | The code check says skip the bus. A vehicle on `Layer::World` stays a wall in the bot grid, and bots have no local avoidance | Needs its own layer, avoidance and a rule for anyone in its path. As I read the code, nothing pushes a standing warrior out of a moving box. Not tested |
| Things marked on the map: tractor drop, Rocket flare, Video viral ping, kite spotter, building flush | vehicles.md, guns.md, delhi_culture.md, battle_royale_design.md | The minimap draws only the ring, the next ring and you | One marker feature that nobody has costed. Build it once |
| Cover that blocks sight but not bullets: shamiana, Gulal smoke, Mirchi cloud, Dengue Fogger, crowds, bushes | hiding.md, guns.md, delhi_culture.md | Bot sight stops only at solid colliders. Throwable effects are Fire, Wind, Chakra and Blast | One shared sim rule that the bot ray also tests. Needs a fixed range, as Low quality draws less |
| Noise tools: Stump Mic, Ladi decoy, dog, tumblers, one sim noise event | guns.md, hiding.md | Footsteps are client-only, bots have no hearing and the HUD has no sound marker | All wait for sim-side noise, after plan step 7 |
| Moving people and animals: Kalesh circle, baraat, monkeys, stray dog, idle figures | delhi_culture.md, hiding.md | No people, animals or moving street life exist | The first one is a new kind of agent with online sync. An M size looks low |
| A route to every roof, blocks as mini-levels, barsati room, two routes up to perches | hiding.md, guns.md | Mapped buildings are solid. CP's ring stairs stop at the first floor, and its roof needs a glide. Houses with roof stairs exist only on Chandni Chowk and Hauz Khas | CP needs a new interior kit first. This is not a small step |
| Shots at 200 m | guns.md | Street kit stops drawing at 90 to 160 m by quality, but its colliders stay | Shots can stop on a crate the player cannot see. Fix before snipers |
| 4x and 6x scopes | guns.md | Third-person camera only, shots land about 10 cm right of the crosshair, detail goes by distance, and the wheel is taken | More than a field of view number. Test in game |
| Bike rider swings the bat | vehicles.md | All attacks stop unless the mode is Ground (`sim/combat.rs:129-132`) | Same change as passengers who shoot, 3 to 4 days |
| Hawalat bat duel and melee-heavy street fights | battle_royale_design.md, guns.md | A melee hit pauses that peer's whole simulation for 0.05 s. There is no block or dodge. Melee is left out of loot, and bots use slot 0 only | Recheck the pause first. New melee items are not data-only |
| Tawa stops a bullet, bat knocks back a thrown bomb | guns.md, delhi_culture.md | The shooter's peer decides hits, head hits are a height test, nothing checks facing, and a swing finds warriors only | New rules, not item data |
| Voice as a cheap first step, Metro-voice alerts | delhi_culture.md, indian_games.md | No voice system. All 67 sounds are synthesized. Recording is weeks of work | Ship text first, voice later |
| Low cover at Uncharted heights, "nothing in between" | hiding.md | Crates are 0.94 m and drums 0.95 m, just under the 1.0 m line. Nobody can crouch yet, and the body is 1.76 m | Crouch first, then adjust two sizes |
| First vehicle rules written for Connaught Place | vehicles.md | Only 32% of CP's road metres are 5 m or wider | Fits the Inner Circle, radials and outer roads, not the footpaths |
| Sniper as step 2 of the code check's order | code check, guns.md | The bot reply past 70 m is "best done with the hearing work", which waits for step 7 | Order conflict. A stopgap to test is raising the 70 m limit at `sim/bots.rs:445` |

### Stale or conflicting facts

| Note says | Code says | Fix |
|---|---|---|
| hiding.md: perception may be only a distance check, and step 1 is "Bots respect walls" | Distance plus one ray. Walls already block sight | Step 1 is done. The real gaps are view cone, hearing, memory and non-solid cover |
| guns.md: bots engage at 45 m, or 26 m when chill | 55 m and 35 m (`sim/bots.rs:103,365`). The 70 m grudge limit stands | Use 55 and 35 |
| vehicles.md: no remote smoothing found, and a driven vehicle adds its own state stream | `follow_remote` has SNAP 4 m and COAST 0.25 s. Datagrams decode only `WarriorState` | Start from a ride field on `WarriorState` |
| hiding.md: Palika Bazaar is on the current map | A name over open ground. The converter drops underground buildings | Say "named, not built" |
| Code check: Chandni Chowk is the best driving map | Widths are defaults, and the real road is a daytime no-motor zone | See claim 12 and the places of worship section |

## Smaller errors found along the way

- **`delhi_culture.md`: "Delhi has over 80 lakh cars" is wrong as written.** ThePrint does say this, but Delhi has 87.61 lakh motor vehicles in all, with 59.28 lakh two-wheelers. That leaves about 28 lakh of everything else. Use "vehicles".
- **`delhi_culture.md`: "India has no dedicated law on game content" is out of date.** The Online Gaming Act 2025 and its 2026 Rules now exist, though they mainly target money games.
- **`vehicles.md`: the Rebirth Island story stops too early.** The vehicles came back two days later.

Sources: [ThePrint](https://theprint.in/ground-reports/delhi-parking-murder-court-cases-road-rage/2911224/) (opened); [Millennium Post](https://www.millenniumpost.in/delhi/delhi-vehicle-count-up-79-to-876-lakh-653097) (opened).

## Limits of this check

- The run's shared search limit ran out near the end. After that I only opened pages already found, and no new searching was done. The search for phat-phat dates returned nothing useful.
- Not tested: the PEGI discrimination rule, Cutting Chai as a Mumbai word, "no express parody defence" in Indian law, and a second source for Hitman 2.
- Slang verdicts rest on the web. A Delhi native still needs to check every line, as the notes already say.
- pubg.gs is an unofficial fan database. It decides nothing alone here, and claim 4 shows it can disagree with other sources.
- OpenStreetMap can miss or misplace small shrines, so the places of worship table is a floor, not a full list.
- The legal parts are research, not legal advice. An Indian IP lawyer should still review them.
- Code facts come from reading the working tree, and `sim/bots.rs` was changing during the earlier check, so line numbers can drift.
