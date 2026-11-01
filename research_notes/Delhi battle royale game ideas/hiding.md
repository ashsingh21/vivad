# Hiding places, stealth and urban cover: battle royales and real Delhi

Sourcing key: pages marked (opened) were read directly. All other web citations come from search-result summaries, where the page itself was not opened, so check them before quoting. A few of those summaries did not say which listed page held which claim, and those lines cite the likeliest pages. Repo facts cite file paths in this project (vivad). Many game details below are years old and may have changed.

## 1. How do battle royales handle hiding: bushes, smoke, crouch and prone, footsteps, ghillie suits, and hiding inside objects?

### Takeaway
Hiding is four separate systems: hide-in objects, soft concealment, smoke, and stance plus sound. In each one, a strong hiding tool has a counter, and studios retune that pairing when either side wins too often.

### Cited Findings

**Hide-in objects**
- Fortnite added hiding spots in Chapter 2 Season 1 as hay bales and metal dumpsters. You walk up, press a prompt to enter, and press the same button to leave — [GamesRadar](https://gamesradar.com/fortnite-hideouts); [Inverse](https://www.inverse.com/article/60206-fortnite-hideout-locations-dumpsters-bales-of-hay-haystacks-open-water-challenge)
- The idea lasted and widened. A Chapter 4 Season 4 challenge asked for a haystack, a dumpster and a "flusher", and haystacks are fragile enough to be destroyed — [Sportskeeda](https://www.sportskeeda.com/fortnite/fortnite-hide-haystack-dumpster-flusher)
- A Chapter 6 Season 3 guide lists port-a-potties, dumpsters and haystacks as hiding places. It says a giant bush does not count, because only interactable objects do — [esports.gg](https://esports.gg/news/fortnite/how-to-hide-in-hiding-places-in-fortnite/)

**Soft concealment**
- Fortnite's Bush consumable left in Chapter 2 Season 1 and returned in Chapter 4 Season 1 as a thrown Big Bush Bomb that spawns a bush instantly. It does not match its surroundings, so it works best in grass or woods — [GGRecon](https://ggrecon.com/guides/fortnite-hide-in-bushes-you-threw-down)
- One fan guide says PUBG fixes foliage render distance to be equal for all players. Going prone in thick foliage wins at close range against someone who has not spotted you, but protects far less at range. This is a blog, not official — [Gamertag Mythras](https://gamertagmythras.com/blog/pubg-battlegrounds/pubg-cover-concealment-guide)
- The ghillie suit works best prone in grass or a bush — [PUBG Mobile Wiki](https://pubgmobile.fandom.com/wiki/Skins); [Mein-MMO](https://mein-mmo.de/en/pubg-tarnanzug,155873/)
- Soft cover such as bushes and grass tends to hide you from NPCs but not from human players — [Level Design Book, cover](https://book.leveldesignbook.com/process/combat/cover) (opened)

**Smoke**
- In Apex, threat optics (1x Digital Threat, 4x-10x Digital Sniper Threat) and Bloodhound's ultimate see through Bangalore's smoke — [Apex wiki](https://apexlegends.wiki.gg/wiki/Bangalore/Mobile)
- That turns smoke into a killbox for a user who has the optic, and a liability if the enemy has it instead — [ProGameGuides](https://progameguides.com/apex-legends/apex-legends-bangalore-guide/); [TheGamer](https://www.thegamer.com/apex-legends-bangalore-tips-guide/)
- Players called for a nerf in Season 18, and in Season 19 Digital Threat was reportedly removed from the crafter to curb the smoke-visibility meta — [GGRecon](https://ggrecon.com/articles/apex-legends-players-call-for-digital-threat-nerf-in-season-18); [Gfinity](https://www.gfinityesports.com/article/apex-legends-digi-threat-removed-season-19)

**Stance and footsteps**
- One PC fan test of PUBG gave these hearing ranges: sprint about 58 m (upright or crouched), upright run about 57 m, crouched run about 24 m, walk about 21 m (upright or crouched), slow crawl 15 m — [Mein-MMO](https://mein-mmo.de/en/enemies-can-hear-your-steps-in-pubg-from-this-distance,307161)
- Another report puts standing run at 56 m, up from 40 m before an update — [GameRant](https://gamerant.com/pubg-stealth-kill-hard-update/)
- A PUBG Mobile guide says crouching or walking is heard within 40 m, which contradicts the PC test — [GuruGamer](https://gurugamer.com/mobile-games/pubg-mobile-sound-tutorial-14494)

**What Panga has today (repo)**
- A `CROUCH` input bit is defined at `crates/shared/src/sim/mod.rs:62`. A grep of `crates/shared/src/sim` and `crates/game/src` found no other use of it.
- Bots hold a `sight` pair, (55, 9) m normally and (35, 7) m for calm bots, used as engage range — `crates/shared/src/sim/bots.rs:46,103,365`. A name search of `bots.rs` found no hearing, noise, raycast or line-of-sight logic. I did not read the whole file.
- Footsteps exist as client audio, read from `crate::animation::Footstep` — `crates/game/src/audio.rs:22,185`.
- Parked autos, thelas, chai stalls and rooftop life already exist as street props (commit 45cad37).

### Inferences
- **Bots decide whether hiding works at all.** If perception is only a distance check, no wall or bush hides anyone from a bot yet. Fixing that comes first, and it is a real opening: soft cover fools NPCs, so even simple bushes and tents can pay off against bots.
- **Hide-in props are the best value for a tiny team.** Fortnite's version is one prompt in and one prompt out, so one code path could serve many Delhi props, and several props already exist.
- **Copying PUBG's sound numbers would be too loud.** A 57 m radius covers about 10,200 m², roughly 4% of Panga's 250,000 m² map. Fifteen running players could cover up to about 60% of the map before overlap. As a starting guess to playtest: run 30 m, walk 15 m, crouch 8 m.
- **Skip prone for now.** Prone's value in these sources comes mostly from grass, while Connaught Place is largely paved outside its parks (the map data lists 48 mapped trees, in `assets/data/maps/delhi_cp.json`, and the generator plants more in the parks). Crouch alone may cover most of the need. A ghillie suit has the same problem on this map.
- **Sound needs to move into the sim.** Footsteps are client audio today, so bots cannot react to them.
- **Smoke fits an existing item.** An Anaar could also leave a short smoke cloud. Keep one counter, as Apex does, and expect to retune it.

### Gaps
- The brief says Apex and Warzone have hiding in lockers or dumpsters. I could only confirm hide-in objects for Fortnite. I ran no dedicated search for the other two, so treat that part of the premise as unverified.
- What a player can do inside a Fortnite hide (see, shoot, be spotted) is unknown. GamesRadar returned cut-off text, and esports.gg and GameSkinny returned 403. Only the headline of GamesRadar's piece on [new dumpsters](https://www.gamesradar.com/uk/fortnites-new-dumpsters-are-turning-players-into-a-dementor) was readable.
- I found no official PUBG figures for foliage cutoffs. The footstep numbers rest on one old fan test. PUBG's [footstep dev letter](https://devtrackers.gg/pubg/p/5c0da102-dev-letter-footstep-sound-rebalance) appeared in results but was not opened.
- Apex stance and sound were not researched.

## 2. How do games stop camping from becoming boring: zones, sound, scans and heartbeat sensors?

### Takeaway
Three layers do the work: a zone that keeps moving, detection tools that give rough information but rarely exact positions, and world noise that punishes careless movement. Campers are never fully safe, and detection is never perfect either.

### Cited Findings

**Zones**
- In late 2019 PUBG tested a second blue circle inside the safe zone, matching the next circle in size and place, so circle campers had to keep moving — [TweakTown](https://www.tweaktown.com/news/69166/beware-pubgs-second-blue-inner-circle-camper-deterrent/index.html); [ComicBook](https://comicbook.com/gaming/2019/12/08/pubg-testing-new-gameplay-feature-to-stop-campers); [The Quint](https://www.thequint.com/tech-and-auto/tech-news/pubg-update-bluehole-mode-coming-soon)
- A patch raised blue zone damage to push engagement inside the play area, and Brendan Greene tied this to stopping players camping far outside and arriving only at the end — [Stevivor](https://stevivor.com/features/interviews/brendan-playerunknown-greene-on-pubgs-latest-update); [Christian Today](https://www.christiantoday.com/news/playerunknowns-battlegrounds-experiments-with-changes-to-the-blue-zone)
- One low-quality post frames PUBG's Black Zone as the thematic opposite of the Red Zone, which drives players into shelter — [Enter Gaming XP](https://entergamingxp.blogspot.com/2020/01/pubgs-new-map-was-designed-to-keep.html)

**Sensors and scans**
- Warzone's heartbeat sensor reaches 50 m, against 30 m in standard multiplayer — [Mein-MMO](https://mein-mmo.de/en/best-tactical-item-in-cod-warzone-heres-why-you-should-use-it-this-is-what-makes-it-strong,496689)
- The Ghost perk hides players from heartbeat sensors and recon drones, which makes it the main counter — [Mein-MMO](https://mein-mmo.de/en/the-strongest-perk-in-cod-warzone-is-broken-this-is-what-you-should-pay-attention-to,491712)
- Streamer Aydan argued Warzone's Ghost should work only while moving, as in Black Ops Cold War, so that a player sitting in a corner loses it — [CharlieIntel](https://charlieintel.com/aydan-explains-how-bocws-ghost-can-stop-warzone-campers/67525)
- A fan proposed a sensor battery that drains with each use. Commenters split on whether it was needed and on how much it would lean on Ghost — [CharlieIntel](https://www.charlieintel.com/call-of-duty-warzone/clever-warzone-heartbeat-sensor-nerf-idea-could-destroy-ghost-meta-141849/)
- Apex's Seer senses nearby enemies through walls while aiming, giving rough direction rather than a precise location — [Inven Global](https://www.invenglobal.com/datalab/articles/14741/guide-how-to-play-seer-in-apex-legends)
- Respawn later nerfed that sensor. A patch also stopped it passing through Catalyst's ultimate — [PCGamesN](https://www.pcgamesn.com/apex-legends/seer-heartbeat-patch-notes)
- Counters include Wraith's Into the Void and Mirage's decoys — [ProGameGuides](https://progameguides.com/apex-legends/best-seer-counters-in-apex-legends/)

**World noise (Hunt: Showdown, the closest model for a Delhi version)**
- The map is full of noise triggers: crows that scatter, kennel dogs, chickens, ducks, horses, hanging chains, broken glass and dry branches — [Early Guides](https://earlyguides.com/hunt-showdown-1896/combat); [ProGameGuides](https://progameguides.com/hunt-showdown/best-tips-for-new-players-in-hunt-showdown/); [KosGames](https://kosgames.com/hunt-showdown-beginners-guide-2021-8865/)
- They tell other players where you are and draw nearby AI to investigate — [Early Guides](https://earlyguides.com/hunt-showdown-1896/combat)
- Crouching through chains or glass makes little or no noise — [ProGameGuides](https://progameguides.com/hunt-showdown/best-tips-for-new-players-in-hunt-showdown/); [KosGames](https://kosgames.com/hunt-showdown-beginners-guide-2021-8865/)
- Skilled players track rivals using crows, dogs and footsteps, and the game's audio is widely praised — [PC Gamer](https://www.pcgamer.com/lets-hear-it-for-hunt-showdowns-incredible-sound-design)

### Inferences
- **The Aandhi already does most of the work.** In a 500 m map over 7 to 9 minutes, every hide expires soon anyway, so Panga needs less anti-camping tech than a large map. The main rule: put hides in spaces the Aandhi will swallow.
- **Preview the next Aandhi.** PUBG's inner circle shows that warning people where to go forces hiders to choose. A thin dust haze over the next closure could reduce or end concealment there.
- **World alarms suit Panga better than gadgets.** They cost little, they add Delhi sound, and they need no menus. Local versions, all my suggestions:
  - Pigeons burst up from a roof when someone runs nearby. Old Delhi's pigeon-flying culture supports this (see section 4).
  - A stray dog barks at a crouched player.
  - Glass tumblers rattle at a chai stall.
  - An auto's horn blares when it is bumped.
- **One sim noise event could serve players and bots.** Hunt's traps alert both. A shared event could drive a sound for players and an investigate goal for bots.
- **If Panga adds a scan, keep it rough.** Give direction only, short range, and limited uses, as Seer's design suggests. A wedding videographer's drone or a chowkidar's whistle would feel local. A 50 m sensor would be large here: from the Inner Circle road it would reach past the 43 m to the back of the blocks.

### Gaps
- Bloodhound's scan was not described in usable detail in my results.
- PUBG's Red and Black Zone purpose rests on one thin source.
- I could not confirm whether the inner blue zone shipped permanently. A [PlayNews guide](https://www.playnews.gg/it/guide/pubg-perche-leggere-i-cerchi-non-funziona-piu-dalla-42-1-la-nuova-zona-blu-spiegata) suggests the blue zone changed in update 42.1, but I did not read it.
- Warzone's current sensor range and Ghost behaviour were not checked against recent patch notes.
- The Hunt sources are fan-written. I found no official trigger list.

## 3. Which real places in and around Connaught Place would make good locations, and how are they laid out?

### Takeaway
Central Park, Rajiv Chowk, the colonnades, rear lanes, rooftops and Palika Bazaar all sit on or under the current map. Jantar Mantar does not, and the Hanuman Mandir and Agrasen ki Baoli are not in the map data, so treat those three as second-map or inspired-by sites. The Hanuman Mandir is an active temple and needs care.

### Cited Findings

**Overall layout**
- Connaught Place was built from 1929 to 1933, with Robert Tor Russell as the designer after W.H. Nicholls left in 1917 — [Wikipedia](https://en.wikipedia.org/wiki/Connaught_Place,_New_Delhi) (opened); [The Tribune](https://www.tribuneindia.com/news/archive/features/so-who-built-cp-it-wasn-t-lutyens-170023)
- Eight roads leave the Inner Circle: Parliament Street and Radial Roads 1 to 7. That explains why some sources say seven and others eight. Twelve roads leave the Outer Circle (Connaught Circus). The Middle Circle holds offices, banks and a PVR cinema — [Wikipedia](https://en.wikipedia.org/wiki/Connaught_Place,_New_Delhi) (opened)
- Blocks are two storeys with an open colonnade: shops below, homes above. The same page says they were first planned at 172 m, which looks wrong, so do not use it — [Wikipedia](https://en.wikipedia.org/wiki/Connaught_Place,_New_Delhi) (opened)
- Sources agree the Inner Circle holds blocks A to F, but the outer range is given as G to N or G to P — [Wikiwand](https://www.wikiwand.com/en/articles/Connaught_Circus); [Arki Students Corner](https://arkistudentscorner.blogspot.com/2011/07/lutyens-delhi-part-2-connaught-place.html). The map data names A to G, L and N — `assets/data/maps/delhi_cp.json`
- The map is a 500 m square round the middle of Central Park, shifted 40 m south — `tools/osm/delhi.py:9,35`. Its park radius is 112 m, the Inner Circle road is at 125 m, and blocks run from 136 to 168 m — `crates/shared/src/map/delhi.rs:92-100`

**Central Park (on map)**
- It is circular, sits at the centre of the layout, and was rebuilt in 2005-06 after the metro was built beneath it. A 90 by 60 ft flag flies on a 207 ft (63 m) pole — [Wikipedia](https://en.wikipedia.org/wiki/Connaught_Place,_New_Delhi) (opened)
- It covers about 41,500 m². Its amphitheatre is quadrant shaped, with about 350 seats and a stage of about 60 m². It has water features and 9 gates, 2 of which lead straight into the metro. One report says a fountain was removed for the metro works — [Delhi Tourism](https://www.delhitourism.gov.in/entertainment/central_park.html); [Commonwealth Walkway](https://commonwealthwalkway.info/place/central-park/); [Explore Our India](https://www.exploreourindia.com/central-park-rajiv-chowk)

**Rajiv Chowk metro (under the park)**
- Levels run street, then UG1 concourse, UG2 Blue Line side platforms, UG3 Yellow Line island platform, with 4 platforms in all. It opened in 2005 — [Wikipedia](https://en.wikipedia.org/wiki/Rajiv_Chowk_metro_station) (opened)
- It has 8 numbered gates. Named exits include B Block / Radial Road 3, PVR Plaza, D Block / Odeon, E Block / Barakhamba Road, N and F Block, Palika Bazaar and A Block / Regal — [Wikipedia](https://en.wikipedia.org/wiki/Rajiv_Chowk_metro_station) (opened)

**Palika Bazaar (on map, south edge)**
- It is underground between the inner and outer circles, with 380 numbered shops and an estimated 15,000 people inside at any time. Dated 1979 — [Wikipedia](https://en.wikipedia.org/wiki/Palika_Bazaar) (opened). Other sources give 1978 and about 425 shops — [Delhi Tourism](https://delhitourism.gov.in/shopping/palika_bazar.html); [The Patriot](https://thepatriot.in/reports/palika-bazaar-from-a-shoppers-paradise-to-a-relic-of-nostalgia-67942/amp)
- It has seven gates. Gate 1 is the main stair, facing Central Park. Gates 2 and 7 are ramps. Gate 3 is closed. Gates 4 and 5 are spiral stairs. Gate 6 is a stair — [Wikipedia](https://en.wikipedia.org/wiki/Palika_Bazaar) (opened)
- Visitors describe a maze of narrow lanes with shops on both sides, and advise picking a gate as an exit landmark — [Trip Hippies](https://triphippies.com/palika-bazar-delhi); [magicpin](https://magicpin.in/blog/palika-bazaar-delhi/)
- It stretches from the Inner Circle junction to the Outer Circle and has underground parking — [Wikipedia](https://en.wikipedia.org/wiki/Connaught_Place,_New_Delhi) (opened). The map places its point 237 m from the park centre (my calculation) and also names Palika Park and Palika Parking — `assets/data/maps/delhi_cp.json`
- One search summary placed it under Central Park, but I could not tie that to a page. Central Park sources call it adjacent — [Delhi Tourism](https://www.delhitourism.gov.in/entertainment/central_park.html). The two Wikipedia pages and the OpenStreetMap-derived position also put it outside the park, so build it beyond the inner blocks.

**Colonnades, rear lanes and rooftops**
- The Inner Circle is a two-storey ring with a colonnaded ground-level passage — [Arki Students Corner](https://arkistudentscorner.blogspot.com/2011/07/lutyens-delhi-part-2-connaught-place.html)
- The Regal back lane is made of building rears, with backstairs and AC units on the walls — [The Delhi Walla](https://thedelhiwalla.blog/2022/05/01/city-walk-regal-cinema-back-lane-connaught-place/)
- CP rooftops today are mainly bars and event venues — [Business Standard](https://www.business-standard.com/article/beyond-business/connaught-place-2-0-114102400830_1.html); [VenueLook](https://www.venuelook.com/delhi/locality/connaught-place/venuetype/roof-top-spaces)
- Shankar Market has more than 100 ground-floor shops, stalls and flats above, and seven corridors — [Delhi Online](https://deals.delhionline.in/city-guide/shankar-market); [Housing.com](https://housing.com/news/shankar-market/amp). It is not named in the map data.

**Jantar Mantar (off map, heritage care)**
- Jai Singh II completed it in 1724. The Samrat Yantra is a triangle about 70 ft high, 114 ft long at the base and 10 ft thick, with a hypotenuse of about 128 ft (39 m). There are hollowed Jai Prakash hemispheres and two open-topped cylindrical Ram Yantras. Wikipedia lists it among Delhi's Monuments of National Importance — [Wikipedia](https://en.wikipedia.org/wiki/Jantar_Mantar,_New_Delhi) (opened)
- It stands on Parliament Street near the NDMC headquarters, and was one of three structures spared in the early-1900s clearance — [Wikipedia](https://en.wikipedia.org/wiki/Connaught_Place,_New_Delhi) (opened)
- From those coordinates it lies about 700 m from the park centre and about 330 m beyond the map's south edge (my calculation).

**Hanuman Mandir (RELIGIOUS SITE, needs care)**
- It stands on Baba Kharak Singh Marg, about 250 m southwest of CP, and is dated to a 1724 rebuild — [Wikipedia](https://en.wikipedia.org/wiki/Hanuman_Temple,_Connaught_Place)
- Tradition counts it among Delhi's five Mahabharata-era temples, which is a belief rather than an established date — [Outlook Traveller](https://www.outlooktraveller.com/experiences/heritage/hanuman-jayanti-2026-6-oldest-hanuman-temples-in-india-you-must-visit)
- It is an active place of worship. Listings differ on hours but all show daily opening, and one lists 24-hour opening on Tuesdays and Saturdays — [Tour My India](https://tourmyindia.com/states/delhi/hanuman-temple-connaught-place.html); [Bhakti Bharat](https://www.bhaktibharat.com/en/mandir/prachin-hanuman-mandir)
- It was one of three structures spared in the early-1900s clearance, along with Jantar Mantar — [Wikipedia](https://en.wikipedia.org/wiki/Connaught_Place,_New_Delhi) (opened)
- The road is in the map data, but no temple is named there — `assets/data/maps/delhi_cp.json`

**Agrasen ki Baoli (not in map data, heritage care)**
- It is a stepwell of about 60 by 15 m, with 103 or 108 steps by different counts. Most sources give three visible levels with arched niches on both sides, and some give four. It has a single straight stair, and the lowest part sits in water — [Wikipedia](https://en.wikipedia.com/wiki/Agrasen_Ki_Baoli); [India Water Portal](https://www.indiawaterportal.org/drinking-water/step-back-time-ugrasen-ki-baoli); [Atlas Obscura](https://www.atlasobscura.com/places/agrasen-ki-baoli)
- It stands in Hailey Lane off Hailey Road and is protected by the Archaeological Survey of India — [Rajasthan Tour Planner](https://www.rajasthantourplanner.com/Delhi-Tour-Planner/Delhi-Monuments/agrasen-ki-baoli-delhi.html)
- A travel blog describes it as haunted — [Tale of 2 Backpackers](https://taleof2backpackers.com/agrasen-ki-baoli/)

### Inferences

| Place | On current map? | Best game role | Care |
|---|---|---|---|
| Central Park | Yes | Open ground to cross, with the amphitheatre as a dip of cover and the flag as a landmark | Avoid disrespectful use of the national flag |
| Rajiv Chowk | Below the map | Fast escape route between blocks | Use a made-up name and no real metro branding |
| Palika Bazaar | Yes, south edge | Close-range maze with six gates | Keep shops made up, like the existing signs |
| Colonnades | Yes | Directional cover around the ring | None |
| Rear lanes | Partly | Flank routes, climbing via backstairs | None |
| Rooftops | Roofs exist | Sniper perches, with noise risk | None |
| Jantar Mantar | No | Second map, or a set of made-up instrument shapes | Protected monument |
| Agrasen ki Baoli | No | Inspired-by sunken stepwell on Palika Park | Protected monument |
| Hanuman Mandir | No | Do not use as a play space | Active temple |

- **Keep the Hanuman Mandir out of play.** My recommendation is no fights in, on or against it, and no sniper perch or loot. If a later map reaches it, show it as a distant, unreachable landmark at most, and seek local advice.
- **Palika's real gates give natural rules.** Spiral stairs make ambush points, ramps make fast exits, and the closed Gate 3 can be a locked door or a surprise route.
- **Sunken open-air spaces are probably far cheaper than tunnels.** `delhi.rs` describes a flat painted ground with water sunk for wading, so a dip in the ground looks possible while a roofed space below ground may need new work. Check this before committing. It favours the amphitheatre and a made-up stepwell before Palika or the metro.
- **The metro could start as a link, not a building.** You enter a gate, fade for a few seconds, and come out at another gate. A loud chime plays at both ends as the tell. Bots would need a matching navigation link. This delivers the escape route without modelling the station.
- **Check for smaller shrines.** The map data is thin on named buildings, so its silence does not prove that no shrine stands in the 500 m square.

### Gaps
- I found no scaled floor plan of Palika Bazaar. Shape, corridor layout and depth are unknown.
- I found no station depth or map tying each Rajiv Chowk exit to a street position.
- Central Park's internal layout (where the amphitheatre, water and flag sit) was not found, nor was it confirmed which fountain exists now.
- The CP rooftop and service-lane search was thin, and I found no survey of roof access. Building heights by block are not sourced.
- Whether Jantar Mantar has climbable stairs, or a shrine on site, is not confirmed by the sources read. The same goes for any small religious structure at Agrasen ki Baoli.
- Other religious sites near CP, such as Gurdwara Bangla Sahib, were not researched. Check them before any map expansion.
- The exact positions of the Hanuman Mandir and Agrasen ki Baoli against the map were not computed, because I had no sourced coordinates. Their "off map" status rests on the data naming neither, and on the source's distance for the temple.

## 4. What everyday Delhi hiding spots feel local?

### Takeaway
Delhi's best hides are ordinary objects players already recognise: roofs with tanks, parked autos, chai stalls, wedding tents, drains and loaded handcarts. Sources are usable on roofs, flyovers and crowded lanes, and thin on tanks, autos and stalls, where the game's own props and the owner's local knowledge will matter more than research.

### Cited Findings
- **Rooftops:** A barsati is a room on top of a house. Families once slept on terraces in the heat and moved into the small rooms in the rains, and Old Delhi roofs were a hub for kite-flying and pigeon-racing. This is a 1990 account — [CS Monitor](https://www.csmonitor.com/1990/0626/pdelhi.html). The word comes from barsaat, rain — [Archinect](https://archinect.com/news/article/143912782/the-disappearing-barsati-or-rooftop-dwellings-of-delhi)
- **Water tanks:** Evidence is anecdotal. One Team-BHP owner reports two 750-litre black tanks lasting over 20 years — [Team-BHP](https://www.team-bhp.com/forum/shifting-gears/100713-home-construction-makeover-maintenance-thread-45.html)
- **Wedding tents:** A shamiana is a South Asian ceremonial tent or awning used for outdoor parties, weddings and feasts — [Wikipedia](https://en.wikipedia.org/wiki/Shamiana). Tent houses across Delhi supply them, including one at Lodhi Colony, and one listing prices a 300-guest setup at ₹50,000 to ₹74,999 with tent, carpet, lighting and seating — [WeddingWire](https://m.weddingwire.in/tent-house/new-lakshmi-tent-house-lodhi-colony--e176379); [WeddingWire](https://www.weddingwire.in/tent-house/dhami-tent-house--e173011)
- **Under flyovers:** These are lived-in spaces. People displaced by evictions and floods sheltered under the Vijay Ghat flyover — [The Quint](https://www.thequint.com/amp/story/news/india/hawkers-banned-delhi-streets-for-g20-yamuna-floodplains-plight). About 40 families under the IIT, RK Puram and Chirag Delhi flyovers were told to leave for the G20 drive in 2023 — [Newslaundry](https://www.newslaundry.com/2023/09/07/g20-beautification-drive-pwd-workers-clean-up-makeshift-homes-under-delhi-flyovers-before-eviction). An adopt-a-flyover policy also covers clearing these spaces — [Swarajya](https://swarajyamag.com/amp/story/infrastructure/under-flyover-zones-in-delhi-set-for-major-makeover-as-iocl-dial-take-up-beautification-projects)
- **Drains:** The Barapullah is described as a smelly nallah, or drain, running under a flyover — [The Delhi Walla](https://www.thedelhiwalla.com/2022/12/24/city-landmark-barahpulla-drain-mathura-road/)
- **Crates and handcarts:** In Khari Baoli, bulk goods move through narrow lanes by handcart and on porters' heads — [Fox News](https://www.foxnews.com/world/for-the-cart-pullers-of-new-delhis-sprawling-spice-market-the-work-has-gone-on-for-centuries); [Maverick Bird](https://maverickbird.com/india/north/delhi/the-sights-and-sounds-of-khari-baoli-spice-market-in-delhi/). Cabs and autos stop at the edge because the lanes are too narrow — [Zingbus](https://www.zingbus.com/blog/chandni-chowk-market-timings-shopping-guide/)
- **Crowds:** Crowding peaks on weekend evenings and before Diwali, Eid and wedding season — [Zingbus](https://www.zingbus.com/blog/chandni-chowk-market-timings-shopping-guide/). Palika Bazaar's estimated 15,000 people is another marker — [Wikipedia](https://en.wikipedia.org/wiki/Palika_Bazaar) (opened)
- **Autos and chai stalls:** I found no external source. Both exist as game props (commit 45cad37).

### Inferences
All entries below are design suggestions. Sizes and tells need playtesting.

| Spot | Type | How it works | Tell or counter |
|---|---|---|---|
| Parked auto | Hide-in, 1 person | Sit in the back with the side flap down | Flap moves on entry, and gunfire damages the auto |
| Rooftop tanki | Hard cover plus small hide | Tank stops sight, and you can crouch behind it | Pigeons lift when you run on the roof |
| Barsati room | Small room on a roof | One door, a window, a sniper's nest | One exit, so a thrown bomb clears it |
| Chai stall | Low cover | About waist high, so you must crouch | Tumblers rattle if you bump it |
| Shamiana | Soft cover | Blocks sight, not bullets | Cloth ripples, and bullets pass through |
| Nallah | Sunken route | Open trench across a block, walk unseen | Wading is slow and splashes loudly |
| Handcart and sacks | Low cover, movable | Push it across a gap, Delhi's own moving cover | Slow to move, and it creaks |
| Stacked crates | Breakable cover | Crates can be shot apart | Temporary by design |

- **Wedding tent as a set piece.** A shamiana with chairs, stage and strings of lights fits a Palika Park or Central Park corner. It is colourful, reads instantly as Delhi, and gives soft cover. A random wedding at match start could add variety between rounds.
- **Crowds are a stretch.** A bazaar's crowd is the most Delhi hide of all, but simulating one is costly, and NPC crowds in a fight raise tone issues. A cheaper version could add a few idle figures or sound only. Hawkers fleeing in panic would need thought.
- **Treat flyovers with care.** Real displaced people shelter there, and evictions are a live issue. A fight under a flyover can use pillars and parked vehicles. It should not use homeless families as set dressing.
- **The nallah is a strong fit for the existing wading.** A drain could reuse the water sinking that `delhi.rs` already mentions, and the Barapullah gives it a real name for a later map.
- **Local names:** tanki, chhat, barsati, nallah, shamiyana, thela. These are ordinary Hindi words. Whoever covers slang should confirm which sound most natural.

### Gaps
- I found no solid source on rooftop water tanks as a cultural feature, and Team-BHP is anecdote. Whether a person could fit inside a tank was not checked, so the table treats tanks as cover only.
- I found no source on parked autos or chai stalls as a street presence. I did not search those two separately, so this is a gap in coverage and not evidence of absence.
- The rooftop account is from 1990. Current Old Delhi roof life was not checked, and kite and pigeon culture was not separately verified.
- Nothing was found on a dedicated tent or pandal street. Drains near CP were not researched.

## 5. How do popular maps design buildings, verticality and cover for good fights?

### Takeaway
Use little cover and place it deliberately, stay within about three floor planes, and make each building or block a small level with several exits. Compact maps feel good because players learn a few named zones like their own home, which suits CP's blocks.

### Cited Findings

**Cover**
- Less cover is usually better, because enemies should often stay partly exposed. Too many wide, tall objects create maze-like flow and hurt wayfinding — [Level Design Book, cover](https://book.leveldesignbook.com/process/combat/cover) (opened)
- Uncharted 4 used fixed heights: high cover at 1.75 m or more, low cover at 1.0 to 1.25 m, and not cover at 0.5 m or less — [Level Design Book, cover](https://book.leveldesignbook.com/process/combat/cover) (opened)
- Gears of War designers preferred low cover in multiplayer, because it keeps players aware of the action — [Level Design Book, cover](https://book.leveldesignbook.com/process/combat/cover) (opened)
- Directional cover has blind spots that enemies can flank, while freestanding cover allows circling duels and suits last stands. Corners of 90 degrees or more protect better than rounded ones — [Level Design Book, cover](https://book.leveldesignbook.com/process/combat/cover) (opened)
- Most cover should be static. Breakable cover is temporary, and moving cover should be slow or very large. Repetitive, evenly spaced boxes can feel artificial — [Level Design Book, cover](https://book.leveldesignbook.com/process/combat/cover) (opened)
- Stealth layouts give players cover as a tool for managing enemies, while shooter layouts give it as protection from fire — [Level Design Book, cover](https://book.leveldesignbook.com/process/combat/cover) (opened)
- The page gives no spacing figures or cover-per-area ratios, so density is a playtest decision — [Level Design Book, cover](https://book.leveldesignbook.com/process/combat/cover) (opened)

**Verticality**
- Limit most areas to about three floor planes, because a fourth adds little. Merge minor height changes — [Level Design Book, verticality](https://book.leveldesignbook.com/process/layout/flow/verticality) (opened)
- Going down is easier than climbing. One-way drops stop backtracking and push players forward. Climbing needs space set aside for stairs, ramps and ladders — [Level Design Book, verticality](https://book.leveldesignbook.com/process/layout/flow/verticality) (opened)
- Competitive shooters often stay fairly flat because height changes disrupt precise aiming. On gamepad, prefer shallow slopes and flat floor planes — [Level Design Book, verticality](https://book.leveldesignbook.com/process/layout/flow/verticality) (opened)

**Compact battle royale maps**
- Rebirth Island cut map size and player count to 40, and fans credit its fast fights — [PC Gamer](https://www.pcgamer.com/au/warzones-rebirth-island-makes-battle-royale-fun-again)
- Players learned its four key zones so well that they felt like home. Fans cite varied flanking paths, verticality that rewards aggression, and recurring bottlenecks — [PC Gamer](https://www.pcgamer.com/warzone-has-streamlined-battle-royale-to-the-point-of-total-transformation); [CharlieIntel](https://www.charlieintel.com/call-of-duty-warzone/jgod-explains-why-warzone-players-enjoy-rebirth-island-more-than-verdansk-137899/)
- A level-design interview in the results treats good areas as mini-levels, where players entering must immediately think about exits, entrances and lines of fire. It says Rebirth's prison came closest but was too large, with openings that were sometimes small — [GamesRadar](https://www.gamesradar.com/warzone-fortunes-keep-best-map); [Inverse](https://inverse.com/gaming/warzone-fortunes-keep-map-review) (the exact page is uncertain, see Gaps)
- Fortune's Keep adds multilevel buildings with ladders and balconies, windows opening onto raised back alleys, ziplines and redeploy balloons — [Inverse](https://inverse.com/gaming/warzone-fortunes-keep-map-review)

### Inferences
- **Three planes fit CP exactly:** below ground (metro, Palika, sunken spaces), street, and roof. That matches the three-plane guidance, so resist adding a fourth.
- **Build props to Uncharted's heights.** Panga is third person like Uncharted 4, so the ranges are a fair start. Chai stalls, crates and handcarts at 1.0 to 1.25 m. Autos, tanks and tent walls at 1.75 m or more if they are meant to hide a standing player. Nothing should land in between, because players should read a prop's purpose at a glance. Measure the current props against this.
- **Each lettered block can be a mini-level.** Give each block two or three ground entries, one route to the roof, and a rear exit to a lane. Named blocks already exist in the map data, so players can learn them as Rebirth's players learned its four zones.
- **Use real features as climb routes.** The Regal lane's backstairs and AC units suit the Fortune's Keep pattern of back alleys joined to upper floors.
- **Treat Central Park as a crossing problem.** It is about 224 m wide by the repo's radius (close to the sourced area: 39,400 m² against 41,500 m²). With snipers added, an empty park becomes a death zone. Use the amphitheatre, tree clumps and a wedding tent as stepping stones, keeping them sparse so the crossing still feels risky.
- **Break up the colonnade's rhythm.** Real columns repeat evenly, which is what the book warns about. Stalls, shutters and thelas can break the pattern without losing the look.
- **Panga is a shooter first.** Keep hides few. As a guess to playtest, start at about one hide-in prop per player (15 on the map), since no source gives a density.
- **A possible build order, given the project's one-step-at-a-time rule.** Prove each step in the real game before the next:
  1. Bots respect walls and line of sight.
  2. Crouch works, with sim-side noise.
  3. One hide-in prop, the parked auto.
  4. One world alarm, pigeons on roofs.
  5. Rooftop access on one block.
  6. Sunken spaces, then the Palika or metro layer.

### Gaps
- I found no GDC talk or developer write-up on Erangel or PUBG map design. A Tencent talk on [Delta Force PvP level design](https://gdcvault.com/play/1035407/Tencent-Games-Developer-Summit-Large) appeared by title only.
- The mini-level interview appeared in a search summary without a clear page, so it needs confirming before anyone quotes it. Its speaker was given only as Jon.
- Rebirth Island's current status is unclear. A [2026 piece](https://playday.one/2026/02/18/the-rebirth-of-rebirth-island-how-a-warzone-favourite-got-a-big-refresh/) suggests a big refresh, but I did not read it.
- I found no numbers for sightline length, building density or doors and windows per building.
- Third-person corner peeking was not researched. It matters because a third-person camera can let a hidden player see around cover without exposing themselves, which could undercut hiding and fairness in Panga. This should be the next question.
- Phone play was not researched. Sound cues may need a visual indicator there.
