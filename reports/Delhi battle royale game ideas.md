# Put Delhi Into Panga's Rules, Not Skins

Big battle royales win with one clear idea, and Indian games have mostly stopped at names and skins. Indus even moved to a Brazilian favela on the same engine ([Outlook Respawn](https://respawn.outlookindia.com/esports/esports-originals/what-happened-to-supergamings-inr-25-crore-tournament)). So Panga should put Delhi into the rules of play. The first steps are cheap: **Delhi words take 1 to 3 days, a sniper takes 2 to 4 days and crouch takes 3 to 5 days.** One drivable auto takes 2 to 3 weeks, and four vehicles take 3 to 5 weeks. A tractor should be one rare, funny event, because nobody needs a vehicle on a 500 m map. **One decision comes before all of this: real places of worship already stand on the rotation maps.** Gurdwara Sis Ganj Sahib is 67 m from the centre of the Chandni Chowk map. Nothing below has been built or tested in a match, and a Delhi native must check every line of slang.

**How to read this report.** Small means days, Medium means weeks and Large means months, for one developer working with AI. Sizes come from the code check, which read the code but built nothing. "(est.)" means the code check gave no size, so the size is my own estimate from its facts. `sim/` means `crates/shared/src/sim/`. The legal parts are research, not legal advice. Evidence is thin on Delhi slang and on what Indian players want. No trusted Delhi slang dictionary was found, and many sources were seen only as search extracts.

**The plan moved today, so some notes are out of date.** `docs/PLAN.md` was edited at 09:45 on 1 November, just after the code check. **Phase 0 is now the current focus: one fun 8-minute match on Connaught Place (CP) against bots.** P1 (bots that walk) and P2 (match pacing) are done. P3 (weapon feel), P4 (loot), P5 (front door) and P6 (playtest with strangers) are open. Phase 1 is paused. I checked the code and updated three things. The ring now runs 520 s, not 450 s (`sim/zone.rs`). Bots already spread their landings, so that idea is done. The notes' "wait for step 7" now means "build on P1 and P2". Step 7 is still unticked, so the owner should confirm this.

## Decide first: real holy places already stand on the maps

Two notes say never to fight at real places of worship. Nobody had checked the maps. All four Delhi maps come from OpenStreetMap, and all four are in the rotation. A check on 1 November compared each 500 m map with [OpenStreetMap](https://overpass-api.de/). I also confirmed the named sites in the map files.

| Map | Site | In the game today |
|---|---|---|
| Chandni Chowk | **Gurdwara Sis Ganj Sahib**, where Guru Tegh Bahadur was executed ([Wikipedia](https://en.wikipedia.org/wiki/Gurudwara_Sis_Ganj_Sahib)) | Solid unnamed block, 17.8 m tall, 67 m from the map centre |
| Chandni Chowk | Shahi Sunehri Masjid | Named in the map file as a `mosque`, with a red sandstone look |
| Chandni Chowk | Jain Sweitamna Temple | Place name on screen |
| Chandni Chowk | Central Baptist Church | Solid unnamed block |
| Hauz Khas | Hauz Khas Medresse Mosque, Feroz Shah Tughlaq's tomb | The mosque is a named block. Madrasa and tomb labels show on screen |
| Hauz Khas | New City Delhi church | Plain shop block, name shows on screen |
| Connaught Place | Moti Masjid, B Block | Plain 11 m block. Its real state today is unclear ([Rana Safvi](https://ranasafvi.com/moti-masjid/amp/), [Sarson Ke Khet](https://sarsonkekhet.in/2018/03/30/a-connaught-place-mosque-is-no-more/)) |
| India Gate | None found. India Gate is a war memorial | One of the two built landmarks |

**Sis Ganj is the sharpest risk.** Hitman 2 had to change a shipped game after Sikh youth objected to a temple level that looked like the Golden Temple ([Hinduism Today](https://www.hinduismtoday.com/hpi/2002/11/21/2002-11-21-sikh-kids-force-changes-in-video-game/)). This list is also a minimum. OpenStreetMap can miss small shrines, and it shows unnamed temples with no matching block. Nobody has checked if chests or floor loot land beside these sites.

The notes give four options and do not choose. They are: no loot or on-screen names near these sites, no landing on them, a review by local Sikh and Muslim players, or a new map. **My advice is to do the first three now, and to replace Chandni Chowk if the review says so.** Do this before P6, because P6 puts the game in front of strangers. Map rotation is also on by default. Removing names and loot near a site is Small (est.). A new map is Small to generate and Medium to bring up to CP's standard.

Item names need the same care. The Sudarshan Chakra's lore names Vishnu, and Trishul Carbine, Vajra-9 and Brahmastra also use sacred names (`assets/data/items.ron`). I found no complaint about such a gun name, so this is a judgement, not a precedent. But protests have hit gods in combat games before ([Engadget](https://www.engadget.com/2012-06-27-religious-group-calls-for-removal-of-hindu-deities-from-hi-rezs.html)), and Panga is now a street game. Renaming into the firecracker family, such as Chakri, is Small (est.). Never make gods or saints into characters. Never use a turban or skullcap as the sign of an enemy. Keep slang away from revered figures too: Hanuman's street-style lines in Adipurush caused a backlash and a rewrite ([Onmanorama](https://www.onmanorama.com/entertainment/entertainment-news/2023/06/18/adipurush-movie-hanuman-dialogue-social-media-backlash.html), [Onmanorama](https://www.onmanorama.com/entertainment/entertainment-news/2023/06/22/adipurush-movie-controversy-dialogues-hanuman-revised.html)).

## Hit games own one clear idea, and Indian games mostly stopped at the surface

Each hit can be said in one line. Free Fire sells 10-minute matches, 50 players and light play ([App Store](https://apps.apple.com/pk/app/id1300146617?l=en-gb)). Warzone turns your first death into a fight ([Mein-MMO](https://mein-mmo.de/en/the-best-new-battle-royale-idea-from-cod-warzone-is-the-gulag,479159/)). BGMI sells itself as Indian culture ([Digit](https://www.digit.in/features/gaming/bgmi-4-3-update-krafton-indias-srinjoy-das-reveals-secret-to-games-success.html)). Panga's line could be **8 minutes, 16 players, real Connaught Place.**

| Game | What it does | Delhi version for Panga | Size |
|---|---|---|---|
| PUBG | Since June 2026, zone damage grows with time spent inside it, not with distance ([PUBG Dev Letter](https://pubg.com/news/10280)) | The Aandhi hurts more the longer you stay outside | Small (est.) |
| Warzone | The Gulag: win a 1v1 after your first death and come back. Players asked for the simple 1v1 to return ([GamesRadar](https://gamesradar.com/warzone-2-is-bringing-back-1v1-gulag-and-players-are-thrilled)) | The Hawalat: a 1v1 bat duel in a tiny room, early in the match | Medium (est.) |
| Fortnite Blitz | 32 players, about 5 minutes, random kit. An unofficial tracker showed a peak near 440,000 players ([Automaton](https://automaton-media.com/articles/newsjp/fortnite-20250710-348661/), [Shacknews](https://www.shacknews.com/article/144783/fortnite-blitz-royale-mode)) | One random rule per match, set by the match seed | Medium (est.) |
| Apex Straight Shot | Restart right from the death screen ([EA](https://www.ea.com/da/games/apex-legends/apex-legends/news/fifth-anniversary-collection-event)) | Already in Panga offline: "R dabao, phir se panga lo". Protect this speed | Done |
| Free Fire | Names hiding as a selling point ([App Store](https://apps.apple.com/pk/app/id1300146617?l=en-gb)) | Make hiding a headline feature | See the hiding table |
| Fortnite | One key to enter a dumpster or hay bale, one key to leave ([GamesRadar](https://gamesradar.com/fortnite-hideouts)) | Hide inside a parked auto | Small to medium (est.) |
| Hunt: Showdown | Crows, dogs, chains and glass make noise that gives you away ([Early Guides](https://earlyguides.com/hunt-showdown-1896/combat)) | Pigeons burst up from a roof when someone runs | Small (est.), after bots can hear |
| BGMI | Local flavour sits on skins and places, with real brand deals ([Digit](https://www.digit.in/features/gaming/bgmi-4-3-update-krafton-indias-srinjoy-das-reveals-secret-to-games-success.html)) | The same layer, with made-up brands and shops | Small per piece |
| Rebirth Island | A small map that players learn like home ([PC Gamer](https://www.pcgamer.com/warzone-has-streamlined-battle-royale-to-the-point-of-total-transformation)) | Show CP's lettered blocks as named zones | Small (est.) |

Indian games teach a second lesson. Critics called Indus's culture mostly cosmetic, with characters who had no abilities or stories and a dull map ([India Today Gaming](https://www.indiatodaygaming.com/reviews/story/indus-battle-royale-review-an-ambitious-desi-spin-on-the-genre-but-can-it-keep-up-5951)). FAU-G was compared badly with PUBG and review-bombed ([Wikipedia](https://en.wikipedia.org/wiki/Fearless_and_United_Guards)). **So Indian players give no discount for being Indian.** Gun feel and smooth play come first, which matches step P3.

Mumbai Gullies shows the scope risk. Its trailer excited many players, but work stopped in May 2026 with no public build. Its founder's lesson was "scope smaller, ship faster, protect your runway" ([Outlook Respawn](https://respawn.outlookindia.com/gaming/gaming-originals/mumbai-gullies-the-rise-and-fall-of-indias-most-hyped-game)). A 500 m map is the right size for a small team. In a developer's own playtest, nearly 90% called Indian settings a standout feature, though no sample size is given ([The Tech Panda](https://thetechpanda.com/fau-g-dominations-recent-playtests-reveal-what-india-wants-from-made-in-india-fps-games/41221/)).

Do not copy attachments, a cash economy, complex abilities or anything that needs a human director. Pings can wait, because matches are all against all today.

## Delhi words and sounds are the cheapest win, and jokes must stop before violence

### Slang: rude without swearing

Today the game has one kill verb, "tapka diya". All 16 item lore lines are loaded, but no code shows them. Text stays in Roman Hinglish for now, because the game has no Hindi font.

The funniest clean lines are Haryanvi. They are rude without any swearing ([Tribune, 2025](https://www.tribuneindia.com/news/musings/anything-but-polite-411803/amp), [Tribune, 2016](https://www.tribuneindia.com/news/archive/musings/punharvi-a-fun-mix-274978)). Delhi also has more than one voice: office Hinglish in CP, Punjabi-tinged Hindi, Haryanvi, and the old Karkhandari speech of Old Delhi ([Dawn](https://www.dawn.com/news/1352112)). Tie a voice to a job or an area, never to a caste or faith.

| Moment | Draft line | Meaning |
|---|---|---|
| Enemy seen | Bhai, scene kharab hai | Bro, this looks bad |
| Long kill | Wo kata! | Cut! (kite cry) |
| Close kill | Chop! | A slicing taunt |
| Same enemy keeps chasing | Chep lag gaya peeche | A clingy one is on me |
| Third player arrives | Aaja, teri e kami reh rahi thi | You were all we needed |
| Ambush | Teri baatt dekhun tha | I was waiting for you |
| Taunt | Ghana matt na bole | Don't talk so much |
| You go down | Lag gaye | I am in trouble |
| All goes wrong | Kaand ho gaya | It has all gone wrong |

The words come from sources ([Intrepid](https://www.intrepidtravel.com/adventures/how-to-sound-like-a-local-in-india/), [Madhavan](https://mrm.substack.com/p/the-internet-personified-a-small)), but the sentences are drafts. There are no teams, so squad calls are out. These lines are solo taunts.

**Do not ship "Bo kata!" yet.** The sources for that exact form come from Lahore ([British Council](https://filmsandfestivals.britishcouncil.org/projects/bo-kata), [Dawn](https://www.dawn.com/news/1969527)). Delhi sources give "Wooh kata" and "Ayi Bo" ([Sunday Guardian](https://latest.sundayguardianlive.com/?p=6023), [The National](https://thenationalnews.com/world/asia/2022/08/14/india-celebrates-75-years-of-independence-with-kite-festival)). The cry itself is real, and it is one of the best local fits for a sniper kill. Use "Wo kata!" as working text, and let a Delhi native choose.

**Word safety matters more than any joke.** Real Delhi street abuse is mostly sex-based, so it cannot be used ([Madhavan](https://mrm.substack.com/p/the-internet-personified-a-small)). Never write mother or sister abuse, or caste or region insults. The ban list is in the research note `delhi_culture.md`. **Chhapri is the main trap.** It is common online, and its exact origin is unproven, but it is widely read as casteist ([FLAME University](https://www.flame.edu.in/in-the-media/digital-exclusion-how-chapri-and-digital-aesthetics-reinforce-caste-hierarchies)). Keep it banned. PEGI lists discrimination only at 18, so a stereotype can cost more than a swear word ([PEGI](https://pegi.info/what-do-the-labels-mean)). The rule is simple: mock ego, never identity. Parking pride and VIP swagger are fair targets. Caste, faith, birthplace, gender and body are not. A preset quick-chat wheel also avoids a chat filter for now. "Cutting Chai" may be a Mumbai word ([Wikipedia](https://en.wikipedia.org/wiki/Masala_chai)), so ask a native if plain "Chai" is better.

### Streets: sound and signs do the most for the least

Today the streets are quiet. The only ambient sounds are day and river. There are no horns, vendors, crowds, people or traffic.

The best single idea is a Metro-style Aandhi alert. Everyone knows the Metro voice ([Zee News](https://zeenews.india.com/viral/darwaje-bayein-taraf-khulenge-meet-the-people-behind-voices-you-hear-in-delhi-metro-shammi-narang-rini-khanna-2649814.html)). Ship it as text first, because voice takes weeks. Later, hire a voice and write new lines. Never clone the real voices.

Truck art is ready to use: "Horn OK Please" and "Dheere chal Pyaare" ([91trucks](https://www.91trucks.com/news/stories-behind-horn-ok-please-funny-quotes-behind-the-trucks-and-their-meanings)). Once autos exist, an auto can answer "Meter nahi chalega" or "Night charge bhi lagega" in the last circle. Both come from real fare fights ([The Quint](https://www.thequint.com/amp/story/videos/news-videos/why-delhi-autowalas-charge-extra-high-autofares), [Deccan Herald](https://www.deccanherald.com/archives/night-charge-bhi-lagega-even-2172726)). Mock the haggle, not the driver. Only one real hawker call was found, so record your own, with consent ([Scroll](https://scroll.in/article/722988/scroll_in)).

### Fights: keep the pettiness, cut the harm

Real Delhi fights start over tiny things: a blocked gate, a scratch, "my spot". Police calls about parking violence rose from 2,413 in 2021 to over 7,000 in 2025 ([ThePrint](https://theprint.in/ground-reports/delhi-parking-murder-court-cases-road-rage/2911224/)). Some of these fights end in death. So the joke must stay on how small the cause is, never on the hurt.

The funny ladder has four steps: a petty trigger, loud pride, "Bhai ko bula", then a crowd with phones up. Stop there. End with a comic exit, such as a mother's phone call, or the Aandhi arriving. Use no real cases and no gang names. Show no slaps at drivers and no violence against women as a joke. The swagger always loses, so "Do you know who I am?" plays when the speaker goes down.

The bat is the only melee weapon today, with no block or dodge. Joke weapons last when they have a real use. PUBG's Pan blocks bullets ([PUBG wiki](https://pubg.wiki.gg/wiki/Pan)), and Indus had a chappal called Maa ka pyaar ([India Today Gaming](https://www.indiatodaygaming.com/reviews/story/indus-battle-royale-review-an-ambitious-desi-spin-on-the-genre-but-can-it-keep-up-5951)). New melee items are not data-only: melee is left out of loot, and bots use only slot 0. A melee hit also pauses that computer's whole game for 0.05 s, so recheck this first.

| Idea | What it does | Size |
|---|---|---|
| Show item lore | Show the 16 hidden lines at the pickup prompt | Small: 1 to 3 days, shared with the next two rows |
| More kill lines | Lines by weapon and cause, such as "X ne Y ko chop kiya". Softer text such as "Mummy ka phone aa gaya" is the owner's taste call | Shared, as above |
| Fix old text | Replace "Garuda pankh kholo" and the English lore | Shared, as above |
| One text file | All lines in one place, so a native can read them | Small: 2 to 3 days |
| Quick-chat wheel | Preset solo taunts online | Small: 2 to 3 days |
| Metro-style alert | "Kripya dhyan dein. Aandhi Rajiv Chowk ki taraf badh rahi hai." | Small as text, Medium as voice |
| Truck art and signs | New painted pieces, such as "Gate ke aage gaadi na lagayein" | Small: hours to a day each |
| Street sound | Horns, vendors and crowd | Small per sound, more if recorded |
| 15 August sky | A sky full of kites. It is not religious, so it is the safest festival | Small (est.) |
| Lathi, hockey stick | New melee items with more reach | Small: 1 to 2 days each |
| Thrown chappal | A tiny blast with its own small sound | Small: 1 to 2 days |
| Hawalat duel | A second chance, open only early in the match | Medium (est.) |
| Block or dodge | New moves | Medium |
| Bat knocks back a bomb, Tawa stops a bullet | New rules, not item data | Medium (est.) |
| Hindi script | Needs a bundled font | Small to medium |
| Voice barks | The system is 2 to 3 days. Recording is the real cost | Medium: weeks |
| Kalesh circle, baraat, monkeys | The first moving person or animal is a new kind of agent with online sync | Large (est.) |

## On a 500 m map, vehicles are for fun, not travel

Nobody needs a vehicle to beat the ring. A sprint crosses the map in about 68 s, and 14 m/s saves only about 30 s. PUBG's Haven and Karakin have no motor vehicles, though a folding bicycle can spawn ([pubg.gs Karakin](https://pubg.gs/maps/karakin/tables/), [pubg.gs Haven](https://pubg.gs/maps/haven/tables/), [RealSport101](https://realsport101.com/article/pubg-151-patch-notes-release-date-mountain-bike-christmas-collab)). But small maps do not always drop them. Livik is the same size as Karakin and has vehicles ([TimesLIVE](https://www.timeslive.co.za/news/gamers-live/2020-07-07-pubg-mobile-gets-15-minute-livik-map/)), and Rebirth Island keeps them too ([Blizzard](https://news.blizzard.com/en-gb/article/24250352/warzone)). CP is a quarter of Haven's area. **So a vehicle here is a choice, and its job is moving cover, loud risk and a funny moment.**

The owner's wish has real support. Players of Indian Bike Driving 3D still look up a tractor cheat code in 2026 ([Game Rant](https://gamerant.com/indian-bike-driving-3d-cheat-codes/), [Cashify](https://www.cashify.in/indian-bike-driving-3d-cheat-codes)). Delhi is also a two-wheeler city: 59.28 lakh of its 87.61 lakh motor vehicles have two wheels ([Millennium Post](https://www.millenniumpost.in/delhi/delhi-vehicle-count-up-79-to-876-lakh-653097)). So the scooter is as Delhi as the auto.

Many games share a rough pattern: fast rides leave you exposed, and tough rides are slow and loud. Speed gaps differ a lot by game, so do not copy one game's numbers. PUBG's Tukshai auto has a boost and a horn, and its release slipped for extra tuning ([PUBG wiki](https://pubg.wiki.gg/wiki/Tukshai)). Free Fire's Tuk Tuk is quiet, fragile and good in narrow streets, and its Monster Truck comes only by airdrop, which makes it an event ([Vandal](https://vandal.elespanol.com/en/free-fire/free-fire-vehicles-how-to-get-them-pros-and-cons.html)). Apex's driver cannot shoot, but side passengers can ([Apex wiki](https://apexlegends.wiki.gg/wiki/Trident)). Do not tune from Warzone's health numbers. They come from one site with no stated method ([RacingGames](https://racinggames.gg/misc/cod-call-of-duty-warzone-vehicle-vehicles-verdansk-chopper-helicopter-suv-bertha-atv-quad-tac-rover-health-speed)).

**These are the cars to keep.** Speeds and health are first guesses to test, not research. Health was set against Tamancha fire of 80 damage per second. All names here are shapes for team notes only.

| Vehicle | Test speed | Test health | Fits | Why keep it | Size |
|---|---|---|---|---|---|
| Auto | 13 m/s | 300 | 5 m roads and wider | First choice. Horn on a key. Open sides expose the driver | Small to medium: 1 to 2 weeks offline, about 1 more online |
| Scooter | 15 m/s | 120 | Also the 2.5 m paths | The everyday Delhi ride. Rider stays exposed | Small: 1 to 3 days |
| Heavy bike (Bullet shape) | 17 m/s | 150 | Same as scooter | Loud. Its backfire sounds like a gunshot | Small: 1 to 3 days |
| Open jeep (Gypsy shape) | 14 m/s | 500 | 5 m roads and the park | Best first car, because the open top keeps the driver hittable | Small: 1 to 3 days |
| Small family car (Maruti 800 shape) | 15 m/s | 400 | 5 m roads and wider | The comedy car. Gives solid cover | Small: 1 to 3 days |
| Tractor | 9 m/s | 1,000 | 7 m roads and parks | One rare event | Small: 1 to 3 days |
| Later: status SUV, tempo, e-rickshaw, phat-phat, white sarkari car with a red beacon | | | | Each adds a joke, not a new role | Small: 1 to 3 days each |
| Skip: bus | | | | Needs its own layer, bot avoidance and a rule for anyone in its path | Medium (est.) |
| Skip: bots that drive | | | | Not needed | Large |

**Rules for CP.** Start with 2 to 3 vehicles at seeded kerb spots. They must be new separate objects, because parked autos are merged into the scenery and cannot be taken. Use no fuel: a Warzone-size tank would last nearly 6 map crossings. Build a driver seat only, because there are no teams. Cap ram damage at about 40 to 80, so a ram finishes a fight but does not replace guns. Only 32% of CP's road metres are 5 m or wider, so keep to the Inner Circle, the radial roads and the outer roads. Stay out of colonnades and shops. Rebirth's vehicles were switched off for two days in April 2022 after players used them to glitch into vaults ([GamingBible](https://gamingbible.com/news/call-of-duty-warzone-has-disabled-vehicles-and-fans-are-livid-20220412), [Charlie Intel](https://charlieintel.com/warzone-april-13-patch-notes/171248)).

**What the code needs.** Build a simple arcade car moved by code, not real wheel physics. The driver's computer should own the vehicle, which fits how Panga already works. Give vehicles their own layer, or a moved vehicle stays a wall in the bots' walking map. Vehicle health, smoke and wrecks are Small to medium, up to 1 week. Prove one auto is fun offline before any online work. It is not yet proven that a vehicle slides cleanly along buildings.

**Chandni Chowk is not the best driving map.** The code check called it that, but its road widths are defaults, not a survey. The real main road has been a no-motor zone from 9 am to 9 pm since June 2021 ([India TV](https://www.indiatvnews.com/news/india/delhi-chandni-chowk-road-notified-as-non-motorised-zone-712405)). A 2023 report says the ban is openly flouted ([Hindustan Times via Magzter](https://magzter.com/fr/stories/newspaper/Hindustan-Times/MOTOR-VEHICLE-BAN-OPENLY-FLOUTED-IN-CHANDNI-CHOWK)). The map also holds Sis Ganj. Start on CP.

**The tractor is a joke with a serious history.** It means rural pride and joy, such as tochan pulling contests and wedding stunts ([VICE](https://vice.com/en/article/tractor-stunts-gone-wrong-are-killing-people-in-india)). It is also the symbol of the 2020 to 2021 farmers' movement. On 26 January 2021 tractor convoys entered Delhi, and some allegedly rammed barricades ([Wikipedia](https://en.wikipedia.org/wiki/2021_Farmers%27_Republic_Day_protest)). Stunts have killed young men, and Punjab announced a ban in 2023 ([Tribune](https://www.tribuneindia.com/news/punjab/punjab-bans-tractor-stunts-after-29-year-old-dies-in-gurdaspur-20648)).

So use **one tractor only, arriving around minute 4 as a marked event.** Its slow speed is honest. Real tractors top out at 28.5 to 31.3 km/h by variant, about 8 to 8.7 m/s ([Tractor News](https://cdn.tractornews.in/tractors/mahindra-575-di), [Merikheti](https://www.merikheti.com/en/tractor/mahindra-575-di-xp-plus)). A legal e-rickshaw is capped at 25 km/h, or 6.9 m/s ([CMVR extract](https://commerciallawpublishers.com/cdn/shop/files/sample_chapter_cmv_13c382c1-7c7f-47b2-8959-ac926f621bdd.pdf)). A Panga sprint is 7.4 m/s, so a tractor that barely beats a runner is already funny. Keep it away from police, barricades and buses. Use no flags, no convoy and no Republic Day event. Avoid names like "Kisan Andolan", "Delhi Chalo", "Tochan King" and "5911". Do not score wheelies. Start with no trolley.

**Keep the tractor off India Gate until the owner decides.** The notes clash here. The code check says the lawns suit it. The vehicle note says a tractor there reads as politics, and the code check did not weigh the 26 January 2021 images. The marked arrival also needs a map marker, because the minimap draws only the ring, the next ring and you. Many ideas need that marker, so build it once. Nobody has costed it, so I call it Small to medium (est.).

**Use made-up brands.** Under section 29(4) of India's Trade Marks Act, a known brand can have a real claim even on unrelated goods. A key Delhi ruling is Crompton Greaves v V-Guard, from 6 March 2024 ([Cyril Amarchand](https://corporate.cyrilamarchandblogs.com/2025/05/weather-crompton-pebble-and-pebble-are-similar-or-identical/)). Tata v Greenpeace gives a commercial game little cover ([Columbia](https://globalfreedomofexpression.columbia.edu/cases/tata-sons-ltd-v-greenpeace-international/)). US law gives less comfort than it seems. AM General lost its Humvee case in 2020, but the 2023 Jack Daniel's ruling narrowed that test ([Robins Kaplan](https://robinskaplan.com/-/media/pdfs/publications/warzone-clash-when-does-a-video-game-title-cross-the-link-into-trademark-infringement)). Realism inside a game may still be safe, but a brand in a title is not ([Fennemore](https://www.legal500.com/intelligence/united-states/intellectual-property/the-use-of-trademarks-in-video-games-does-realism-serve-as-a-defense-to-infringement)).

So invent a small fake motor world, as GTA does ([Sportskeeda](https://sportskeeda.com/gta/5-famous-vehicle-manufacturers-gta-series-real-life-inspirations)). Use plain words in the game: Auto, Tempo, Phat-Phat, Tractor, Gaadi. Avoid real grilles, badges and model numbers. Words like Bullet, Gypsy, Scooty and Thar may also be marks. Use a made-up bus company and no police emblem. An Indian IP lawyer should review this before launch.

## A sniper costs days, but three fixes must land first

Top rifles stay strong because they are hard to get. PUBG's AWM comes only from care packages and holds 5 rounds, and its head shot still does 118.1 through the best helmet ([PUBG Statistics](https://pubgstatistics.com/weapons/awm), [PUBG wiki](https://pubg.wiki.gg/wiki/AWM)). Apex's Kraber is a crate weapon with 12 shots ([Apex wiki](https://apexlegends.wiki.gg/wiki/Kraber_.50-Cal_Sniper)). Warzone shows the failure case, with years of swings on one-shot rules. Its Ranked mode still keeps one-shot protection on snipers ([GG Clan](https://ggclan.com/news/warzone-season-06-ranked-sr-weapon-changes)). Fortnite's Heavy Sniper pays with one round and a reload of 3.5 to 4 s ([GINX](https://www.ginx.tv/en/fortnite/how-to-get-fortnite-heavy-sniper-rifle-and-stats)). Scope glint matters too: Warzone players called it unfair when it was removed from some scopes ([Mein-MMO](https://mein-mmo.de/en/scopes-of-the-best-sniper-weapons-in-cod-warzone-are-broken-and-really-unfair,666389)).

Panga's rules already allow a sniper from one data entry: damage, drop, headshot, loot and online sync. It still needs a model, a sound, a scope view and four small code lists. **That is Small: 2 to 4 days. But three fixes come first.**

**First, the Gulel.** A full draw does 95 damage times rarity, and double on the head. That is 205 to 236 on a head, so it kills a full 200 health and shield at every rarity it drops. It beats any fair sniper today. Retune it to about 62 and make it the quiet option.

**Second, the bots.** Bots see 55 m, or 35 m when calm, and they answer a hit only inside 70 m (`sim/bots.rs`). Past 70 m a roof sniper gets free kills. The real fix comes with bot hearing. As a stopgap, raise the 70 m limit and test it.

**Third, far cover.** Street props stop drawing at 90 to 160 m by quality, but their solid shapes stay. A shot can stop on a crate the player cannot see. Fix this before long shots. Nobody has costed it, so I call it Small (est.).

Also test the aim. Shots land about 10 cm right of the crosshair, which is close to half a head under strong zoom. Nothing has proven this in a real game yet.

**Rifles matter from about minute 2 to minute 6.** The ring is only 120 m wide at 350 s (`sim/zone.rs`). CP's ring of buildings faces across the park at about 270 m, so CP is a good test. Late fights still need a second gun. CP's ring stairs stop at the first floor, so its roof needs a glide. A route to every roof is not a small job.

| Name idea | Based on | Role | Damage | Head | Magazine | Scope | Drops | Size |
|---|---|---|---|---|---|---|---|---|
| Thri-Not-Thri | .303 rifle | Main bolt rifle | 70 | x2 | 5 | 4x | Rare to Legendary | Small: 2 to 4 days |
| Thaana SLR | SLR | Marksman rifle | 34 | x1.6 | 10 | 2x | Rare, Epic | Small (est.) |
| Haathi Rifle | Made up | Rarest rifle, about 8 shots in all | 75 | x2 | 4 | 6x | Divya only | Small (est.) |
| Gulel (retune) | Slingshot | Quiet charge shot | 62 | x2 | 1 | None | Rare and up | Small (est.) |

These are starting numbers to test one at a time. With them, a normal rifle kills a head through one Lassi at Rare (151 against 150). Only the Divya rifle kills a head through a full 200. Keep bullets at 350 to 450 m/s so a long shot still needs skill. At real rifle speeds a 200 m shot lands in about a quarter of a second.

Fix one scope to each rifle, because there is no attachment system, and do not add one. Joke names can label the rifle variants: Chashma 2x, Doorbeen 4x, Videshi 6x. Test 4x and 6x in the real game, because the camera is third person and detail drops by distance. Give rifles their own ammo, which is Small. All guns share one pool today, so a 30-pack would mean 30 rifle shots. Glint from 4x up, flinch when hit and weak hip fire are Small (est.). The next gap after rifles is a true assault rifle, and Saintalis, Hindi for 47, is a good name. Do not add many guns. A few clear roles beat a long list.

**Thri-Not-Thri is the best local name.** Hindi press writes "3 नॉट 3 राइफल" ([Patrika](https://www.patrika.com/varanasi-news/303-rifle-retire-in-up-police-on-republic-day-2020-5696009/)). Delhi Police planned to dismantle 7,000 old .303 rifles, which gives lore without praising a gang gun ([The Tribune](https://www.tribuneindia.com/news/delhi/delhi-police-to-bid-adieu-to-7-000-british-era-303-rifles-563692)). Teen-Pandrah is left out. Sources say ".315 bore", and that number may point at street kattas ([Lokmat Times](https://www.lokmattimes.com/national/inter-state-illegal-arms-syndicate-busted-three-nabbed/amp)). Skip Glock, MP5 and the Army's "Sher". Indus says all its guns are made up ([Pocket Gamer](https://www.pocketgamer.com/indus/release-date/)).

Keep the tone comic. Haryana Police removed 67 songs in January 2026 over gun and gang glorification ([Newslaundry](https://www.newslaundry.com/2026/01/15/badmashi-under-scrutiny-haryana-police-removes-67-haryanvi-songs-from-streaming-platforms)). So guns should feel like jugaad, not gangster dreams. Use no real gangs and no wedding firing emote. India's rating standard is the voluntary IS 19690:2026, and IGRS is Indonesia's system ([Outlook Respawn](https://respawn.outlookindia.com/gaming/gaming-originals/indias-first-game-rating-standard-real-need-or-overreach)). My newest source on "voluntary" is from April 2026, so check again before launch. Run no paid-entry cash prizes, because money games are banned ([Bar & Bench](https://www.barandbench.com/law-firms/view-point/playing-by-the-rules-indias-online-gaming-act-and-rules-explained)).

## Hiding works only when bots can be fooled, and every hide needs a counter

Other games split hiding into four parts. Fortnite has hide-in objects, with one key in and one key out ([GamesRadar](https://gamesradar.com/fortnite-hideouts)). Bushes hide you from AI more than from people ([Level Design Book](https://book.leveldesignbook.com/process/combat/cover)). Apex shows that smoke needs a counter, as threat optics see through it ([Apex wiki](https://apexlegends.wiki.gg/wiki/Bangalore/Mobile)). PUBG adds stance and sound: one fan test heard a run at about 57 m and a walk at about 21 m ([Mein-MMO](https://mein-mmo.de/en/enemies-can-hear-your-steps-in-pubg-from-this-distance,307161)). Yet PUBG also added a way to find or force out hiders on both tiny maps: Karakin's Black Zone and Haven's helicopter ([GameRevolution](https://www.gamerevolution.com/guides/628141-pubg-6-1-update-patch-notes-new-map-karakin-pc), [Shacknews](https://shacknews.com/article/121842/pubg-update-101-patch-notes-feature-new-haven-map-an-npc-enemy-faction)).

**In Panga, walls already block bot sight, so that old worry is solved.** The real gaps are different. Bots see all round. They have no hearing and no memory of where you were. Anything without a solid shape hides nobody, so a bush or tent is useless against bots until one new rule exists. Footsteps are only a local sound on each computer, so the game rules never hear them. The minimap shows no other players, so it never gives a hider away.

Some hiding works today: rooms and roofs of houses on Chandni Chowk and Hauz Khas, roofs reached by gliding, and rooftop stair rooms. From the code, bots never enter closed houses, but no match has proven this. The ring ends at radius 0, so hiding cannot win a match alone. Do not copy PUBG's sound ranges. A 57 m circle covers about 4% of a 500 m map. A better first guess is run 30 m, walk 15 m and crouch 8 m.

| Spot | How it works | Counter | Size |
|---|---|---|---|
| Crates and drums | Crouch behind them. At 0.94 m and 0.95 m they sit just under the 1.0 m low-cover line, so raise them ([Level Design Book](https://book.leveldesignbook.com/process/combat/cover)) | Open from the side | Small: 3 to 5 days for crouch, 1 day for quiet steps |
| Bush in a park | A bot sees you only from close | Fixed range on every quality, so Low players cannot see through it | Small: 3 to 5 days |
| Shamiana tent | Blocks sight, not bullets, using the same soft-cover rule ([Wikipedia](https://en.wikipedia.org/wiki/Shamiana)) | Bullets pass through | Small (est.), after bushes |
| Smoke | Gulal Bomb, or a Fogger, which is not religious. Holi needs care, so ask a native ([Millennium Post](https://www.millenniumpost.in/delhi/fear-over-festivity-transpeople-women-face-annual-holi-ordeal-602615)) | Short life | Small (est.), after bushes |
| Rooftop tanki | A new roof prop that stops sight and shots | Pigeons lift when you run, once noise exists | Small: up to a day |
| Colonnade stalls | Stalls, shutters and thelas break the even rows of columns, so cover feels real ([Level Design Book](https://book.leveldesignbook.com/process/combat/cover)) | Open from the side | Small: up to a day each |
| Central Park stepping stones | A few tree clumps and one wedding tent, so the park is not an empty death zone | Keep them few, so the crossing stays risky | Small to medium (est.) |
| House rooms and roofs | Doors that open and close, with bot paths | Doors can be broken | Small to medium: about 1 week |
| Parked auto | A new object and a hidden state | The flap moves, and gunfire hurts the auto | Small to medium (est.) |
| Nallah | A sunken route, reusing the water code | Wading is slow | Small to medium (est.) |
| Made-up stepwell | Bring the old one back from git | Open from above | Small to medium: about 1 week |
| Palika Bazaar | Underground market on CP | The ring ignores height, so it is no shelter | Medium: 2 to 4 weeks |
| Metro link | Enter a gate, fade, leave by another gate. Use a made-up name | A loud chime at both ends. Bots need a matching path | Medium (est.) |
| Force-out event | Mark one building, warn for 20 s, then area damage. A "sealing drive" could be the skin | Needs the map marker | Medium (est.) |
| Prone | Lying down, with new clips and camera work | CP is mostly paved, so skip it | Medium: 1 to 2 weeks |

**The Aandhi is the main counter.** Every hide expires when the ring arrives. Add time-based damage, so nobody can sit outside and out-heal it. Today it does only 1 to 7 damage per second in the first four phases. From reading the code, chai and lassi can beat that, but no match has tested it. No source says how PUBG counts the time, so our rule will be our own design ([PUBG Dev Letter](https://pubg.com/news/10280)).

**Palika Bazaar is named, not built.** The name sits over open ground, because the map converter drops underground buildings. No floor plan appears in anything read, so use its gates and shape as inspiration only. Bots also cannot use stairs that go below the street yet.

**There is no stepwell on CP.** Agrasen ki Baoli is 457 m past the play edge, Jantar Mantar is 347 m past it and the Hanuman Mandir is 204 m past it ([OpenStreetMap](https://overpass-api.de/), [Wikipedia](https://en.wikipedia.org/wiki/Hanuman_Temple,_Connaught_Place)). So use made-up spaces, and keep the Hanuman Mandir out of play on any later map. This does not clear CP, because Moti Masjid is still on it.

Some note ideas do not fit the code. Props are static, so a pushed handcart and crates shot apart are out for now. No rotation map has a bus or tempo, so walk-in vehicles are out. Pigeons, tumblers and other noise alarms wait for bot hearing. Keep hides few, because Panga is a shooter first. About one hide-in prop per player is a guess to test. Stay within three levels: below ground, street and roof ([Level Design Book](https://book.leveldesignbook.com/process/layout/flow/verticality)).

## Conclusion: build these ten things, in this order

Delhi is cheapest in words and sound, but it lasts only in rules. A skin can be swapped, as Indus showed. A kite cry on a long kill, a ring that speaks like the Metro and a tractor that barely beats a runner cannot. The largest risks are not in the code. They are a holy place in a shooter, a caste word in a taunt and a tractor near a barricade. Each costs little to avoid now and a lot to fix after launch.

Phase 0 says the P6 playtest chooses the next phase. So steps 1 to 4 fit before the playtest, and steps 5 to 10 are my default order after it. Let the playtest notes change that order. If players want cars, step 8 can move up. None of this is in `docs/PLAN.md` yet, so add it there. Keep the owner's rule: one step at a time, each proven in the real game. Leave out the bus, bots that drive, prone, fuel, trolleys, pings, attachments, crowds and wedding processions for now.

| # | Build this | Size | Prove it in the game by |
|---|---|---|---|
| 1 | **Decide the holy-site rule and the sacred item names.** Remove names and stop loot near those sites | Small (est.) | No listed site has a name or loot near it in the playtest build |
| 2 | **Delhi words.** Show lore, add kill lines, fix old text, use "Wo kata!", add Metro-style text alerts, and put all text in one file | Small: 1 to 3 days, plus 2 to 3 days | A Delhi native has read every line |
| 3 | **Gun feel.** Finish plan step P3 and retune the Gulel | P3 is in the plan. Gulel is Small (est.) | A head shot no longer kills a full 200 with the Gulel |
| 4 | **Thri-Not-Thri on CP.** Fix far props, raise the 70 m bot limit, then add the rifle with a 4x scope | Small: 2 to 4 days | A bot answers a shot from 100 m, and shots land on the crosshair |
| | **Then run plan steps P4, P5 and the P6 playtest** | In the plan | Strangers play two matches each |
| 5 | **Crouch, quiet steps and bot senses.** View cone, hearing and last-seen memory | Small: 3 to 5 days, 1 day, 2 to 4 days | A crouched player can pass behind a bot |
| 6 | **Bushes, one soft-cover rule, smoke and time-based Aandhi damage** | Small: 3 to 5 days, the rest Small (est.) | A bot loses you in a bush, and nobody out-heals the ring |
| 7 | **Street fight items.** Recheck the melee pause, then add a lathi and a thrown chappal | Small: 1 to 2 days each | Bat fights stay smooth with many hits |
| 8 | **One auto on CP, offline then online.** Then scooter, open jeep and one rare tractor, kept off India Gate until the owner decides | Medium: 3 to 5 weeks | The auto is fun offline before any online work |
| 9 | **Palika Bazaar under CP** | Medium: 2 to 4 weeks | Players and bots can both enter and leave it |
| 10 | **Voice lines** with real Delhi voices, after the text has passed the native check | Medium: weeks | Players keep the lines on and quote them |
