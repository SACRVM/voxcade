//! The engine's own words in the player's language (`c.languages`, the
//! SETTINGS page's LANGUAGE): its menus, hints and the high score table in
//! German and French beside English; a game that speaks others sees them
//! in English. A word is looked up by its English, a number goes where the
//! English has `{}`.

/// The engine's words: English, German, French.
const WORDS: &[(&str, &str, &str)] = &[
    ("CONTINUE", "WEITERSPIELEN", "CONTINUER"),
    ("START", "START", "JOUER"),
    ("SETTINGS", "EINSTELLUNGEN", "OPTIONS"),
    ("QUIT", "BEENDEN", "QUITTER"),
    ("RESUME", "WEITER", "REPRENDRE"),
    ("PLAYER {}", "SPIELER {}", "JOUEUR {}"),
    ("LEAVE", "AUSSTEIGEN", "QUITTER"),
    ("RESTART", "NEU STARTEN", "RECOMMENCER"),
    ("QUIT TO TITLE", "ZUM TITEL", "RETOUR AU TITRE"),
    ("MUSIC", "MUSIK", "MUSIQUE"),
    ("SOUND", "EFFEKTE", "SONS"),
    ("SOUNDTRACK", "SOUNDTRACK", "BANDE SON"),
    ("FULLSCREEN", "VOLLBILD", "PLEIN ÉCRAN"),
    ("SCREEN SHAKE", "BILDWACKELN", "TREMBLEMENTS"),
    ("FLASHES", "BLITZE", "FLASHS"),
    ("BLUR", "UNSCHÄRFE", "FLOU"),
    ("SHADOWS", "SCHATTEN", "OMBRES"),
    ("SHOW FPS", "FPS ZEIGEN", "AFFICHER FPS"),
    ("FRAME RATE", "BILDRATE", "CADENCE D'IMAGES"),
    ("SETTINGS UNREADABLE, THE OLD FILE IS KEPT AS .BROKEN", "EINSTELLUNGEN UNLESBAR, DIE ALTE DATEI LIEGT ALS .BROKEN", "OPTIONS ILLISIBLES, L'ANCIEN FICHIER EST GARDÉ EN .BROKEN"),
    ("SAVE {} CANNOT BE READ", "SPEICHERSTAND {} LÄSST SICH NICHT LESEN", "LA SAUVEGARDE {} EST ILLISIBLE"),
    ("COULD NOT SAVE: {}", "SPEICHERN FEHLGESCHLAGEN: {}", "SAUVEGARDE IMPOSSIBLE : {}"),
    ("COULD NOT LOAD: {}", "LADEN FEHLGESCHLAGEN: {}", "CHARGEMENT IMPOSSIBLE : {}"),
    ("SETTINGS NOT SAVED: {}", "EINSTELLUNGEN NICHT GESPEICHERT: {}", "OPTIONS NON ENREGISTRÉES : {}"),
    ("UNLIMITED", "UNBEGRENZT", "ILLIMITÉE"),
    ("VSYNC", "VSYNC", "VSYNC"),
    ("60", "60", "60"),
    ("30", "30", "30"),
    ("RUMBLE", "VIBRATION", "VIBRATIONS"),
    ("LANGUAGE", "SPRACHE", "LANGUE"),
    ("CONTROLS", "STEUERUNG", "COMMANDES"),
    ("RESET", "ZURÜCKSETZEN", "PAR DÉFAUT"),
    ("BACK", "ZURÜCK", "RETOUR"),
    ("SAVE", "SPEICHERN", "SAUVEGARDER"),
    ("LOAD", "LADEN", "CHARGER"),
    ("PAUSED", "PAUSE", "PAUSE"),
    ("NEW HIGH SCORE!", "NEUER REKORD!", "NOUVEAU RECORD!"),
    ("HIGH SCORES", "BESTENLISTE", "MEILLEURS SCORES"),
    ("HI-SCORE {}", "REKORD {}", "RECORD {}"),
    ("ON", "AN", "OUI"),
    ("OFF", "AUS", "NON"),
    ("HIGH", "HOCH", "HAUTE"),
    ("MEDIUM", "MITTEL", "MOYENNE"),
    ("LOW", "NIEDRIG", "BASSE"),
    ("KEYBOARD", "TASTATUR", "CLAVIER"),
    ("GAMEPAD", "GAMEPAD", "MANETTE"),
    ("PRESS A KEY OR BUTTON", "DRÜCKE EINE TASTE", "APPUIE SUR UNE TOUCHE"),
    ("PRESS START", "DRÜCKE START", "APPUIE SUR START"),
    ("ENTER YOUR INITIALS", "DEINE INITIALEN", "TES INITIALES"),
    ("MOVE: ARROWS OR WASD, LEFT STICK OR D-PAD", "LAUFEN: PFEILE ODER WASD, STICK ODER KREUZ", "BOUGER: FLÈCHES OU WASD, STICK OU CROIX"),
    ("ENTER: SELECT   ESC: BACK", "ENTER: WÄHLEN   ESC: ZURÜCK", "ENTER: CHOISIR   ESC: RETOUR"),
    ("ENTER: CONTINUE", "ENTER: WEITER", "ENTER: CONTINUER"),
    ("TYPE OR UP/DOWN   ENTER: NEXT   ESC: BACK", "TIPPEN ODER HOCH/RUNTER  ENTER: WEITER  ESC: ZURÜCK", "TAPE OU HAUT/BAS   ENTER: SUITE   ESC: RETOUR"),
    ("ESC OR {}: CANCEL", "ESC ODER {}: ABBRECHEN", "ESC OU {}: ANNULER"),
    ("UP/DOWN: LETTER   {}: NEXT   {}: BACK", "HOCH/RUNTER: BUCHSTABE   {}: WEITER   {}: ZURÜCK", "HAUT/BAS: LETTRE   {}: SUITE   {}: RETOUR"),
    ("{}: CONTINUE", "{}: WEITER", "{}: CONTINUER"),
    ("{}: SELECT   {}: BACK", "{}: WÄHLEN   {}: ZURÜCK", "{}: CHOISIR   {}: RETOUR"),
    ("{} PLACE   SCORE {}", "{} PLATZ   PUNKTE {}", "{} PLACE   SCORE {}"),
    ("EMPTY", "LEER", "VIDE"),
    ("AUDIO", "TON", "AUDIO"),
    ("VIDEO", "BILD", "VIDÉO"),
    ("KEYS", "TASTEN", "TOUCHES"),
    ("SWAP PLAYERS 1 AND 2", "SPIELER 1 UND 2 TAUSCHEN", "ÉCHANGER JOUEURS 1 ET 2"),
    ("NONE", "KEINE", "AUCUN"),
    ("ALL", "GANZ", "TOUT"),
    ("LEFT HALF", "LINKE HÄLFTE", "MOITIÉ GAUCHE"),
    ("RIGHT HALF", "RECHTE HÄLFTE", "MOITIÉ DROITE"),
    ("UP", "HOCH", "HAUT"),
    ("DOWN", "RUNTER", "BAS"),
    ("LEFT", "LINKS", "GAUCHE"),
    ("RIGHT", "RECHTS", "DROITE"),
    ("MOVE: LEFT STICK OR D-PAD", "LAUFEN: STICK ODER KREUZ", "BOUGER: STICK OU CROIX"),
    ("JUST NOW", "GERADE EBEN", "À L'INSTANT"),
    ("{} MIN AGO", "VOR {} MIN", "IL Y A {} MIN"),
    ("{} H AGO", "VOR {} STD", "IL Y A {} H"),
    ("{} DAYS AGO", "VOR {} TAGEN", "IL Y A {} JOURS"),
    ("NETWORK", "NETZWERK", "RÉSEAU"),
    ("HOST", "SPIEL ÖFFNEN", "HÉBERGER"),
    ("JOIN", "BEITRETEN", "REJOINDRE"),
    ("JOINING", "BEITRITT", "CONNEXION"),
    ("DROP {}", "{} ENTFERNEN", "RETIRER {}"),
    ("THE HOST'S ADDRESS (ITS PAUSE PAGE SAYS IT)", "DIE ADRESSE DES SPIELS (SEINE PAUSE ZEIGT SIE)", "L'ADRESSE DE L'HÔTE (SA PAUSE L'INDIQUE)"),
    ("UP/DOWN: SIGN   RIGHT: ONE MORE   {}: JOIN   {}: BACK", "HOCH/RUNTER: ZEICHEN  RECHTS: NOCH EINS  {}: LOS  {}: ZURÜCK", "HAUT/BAS: SIGNE  DROITE: UN DE PLUS  {}: REJOINDRE  {}: RETOUR"),
    ("TYPE   ENTER: JOIN   ESC: BACK", "TIPPEN   ENTER: BEITRETEN   ESC: ZURÜCK", "TAPE   ENTER: REJOINDRE   ESC: RETOUR"),
    ("{}: CANCEL", "{}: ABBRECHEN", "{}: ANNULER"),
    ("ESC: CANCEL", "ESC: ABBRECHEN", "ESC: ANNULER"),
    ("ASKING {}", "FRAGE BEI {} AN", "DEMANDE À {}"),
    ("THE TICKET'S HOST", "DEN HOST DES TICKETS", "L'HÔTE DU TICKET"),
    ("THE HOST LEFT", "DER HOST IST GEGANGEN", "L'HÔTE EST PARTI"),
    ("NO ANSWER FROM THE HOST", "KEINE ANTWORT VOM HOST", "PAS DE RÉPONSE DE L'HÔTE"),
    ("THIS MACHINE CANNOT KEEP UP", "DIESER RECHNER KOMMT NICHT MIT", "CETTE MACHINE NE SUIT PAS"),
    ("YOU LEFT", "DU BIST GEGANGEN", "TU ES PARTI"),
    ("RECEIVING THE GAME {}%", "EMPFANGE DAS SPIEL {}%", "RÉCEPTION DU JEU {}%"),
    ("CATCHING UP {}%", "HOLE AUF {}%", "RATTRAPAGE {}%"),
    ("HOSTED AT {}", "OFFEN UNTER {}", "OUVERT SUR {}"),
    ("{} PLAYS", "{} SPIELT MIT", "{} JOUE"),
    ("PAUSED BY {}", "PAUSE VON {}", "PAUSE PAR {}"),
    ("{} LEFT", "{} IST GEGANGEN", "{} EST PARTI"),
    ("{} WAS DROPPED", "{} WURDE ENTFERNT", "{} A ÉTÉ RETIRÉ"),
    ("NO SEAT FREE: YOU WATCH", "KEIN PLATZ FREI: DU SCHAUST ZU", "PLUS DE PLACE: TU REGARDES"),
    ("TYPE AN ADDRESS", "ADRESSE TIPPEN", "TAPER UNE ADRESSE"),
    ("HOST ON THE INTERNET", "IM INTERNET ÖFFNEN", "HÉBERGER SUR INTERNET"),
    ("COPY THE TICKET", "TICKET KOPIEREN", "COPIER LE TICKET"),
    ("LET IN {}", "{} EINLASSEN", "FAIRE ENTRER {}"),
    ("{} ASKS IN, SAYS {}", "{} WILL REIN, SAGT {}", "{} VEUT ENTRER, DIT {}"),
    ("HOSTED ON THE INTERNET", "IM INTERNET OFFEN", "OUVERT SUR INTERNET"),
    ("SEND THE TICKET (COPY THE TICKET)", "TICKET SCHICKEN (TICKET KOPIEREN)", "ENVOYER LE TICKET (COPIER LE TICKET)"),
    ("THE HOST LETS YOU IN: SAY {}", "DER HOST LÄSST DICH REIN: SAG {}", "L'HÔTE TE FAIT ENTRER : DIS {}"),
    ("ON THE INTERNET: CODE {}", "IM INTERNET: CODE {}", "SUR INTERNET : CODE {}"),
    ("SEND THE CODE (COPY THE CODE)", "CODE SCHICKEN (CODE KOPIEREN)", "ENVOYER LE CODE (COPIER LE CODE)"),
    ("COPY THE CODE", "CODE KOPIEREN", "COPIER LE CODE"),
    ("CODE COPIED: SEND IT", "CODE KOPIERT: SCHICK IHN", "CODE COPIÉ : ENVOIE-LE"),
    ("TYPE A CODE", "CODE EINGEBEN", "TAPER UN CODE"),
    ("ANY GAME ON THE INTERNET", "IRGENDEIN SPIEL IM INTERNET", "N'IMPORTE QUELLE PARTIE SUR INTERNET"),
    ("ANY GAME", "IRGENDEIN SPIEL", "N'IMPORTE QUELLE PARTIE"),
    ("THE CODE IS ANOTHER GAME'S", "DER CODE GEHÖRT ZU EINEM ANDEREN SPIEL", "LE CODE EST CELUI D'UN AUTRE JEU"),
    ("THE CODE IS FOR ANOTHER VERSION", "DER CODE IST FÜR EINE ANDERE VERSION", "LE CODE EST POUR UNE AUTRE VERSION"),
    ("NO GAME WITH A FREE SEAT", "KEIN SPIEL MIT FREIEM PLATZ", "AUCUNE PARTIE AVEC UNE PLACE LIBRE"),
    ("TOO MANY ASKS: WAIT A MOMENT", "ZU VIELE ANFRAGEN: WARTE KURZ", "TROP DE DEMANDES : ATTENDS UN PEU"),
    ("THE GAME'S CODE (THE HOST SENDS IT)", "DER CODE DES SPIELS (DER HOST SCHICKT IHN)", "LE CODE DE LA PARTIE (L'HÔTE L'ENVOIE)"),
    ("NO GAME WITH THE CODE {}", "KEIN SPIEL MIT DEM CODE {}", "AUCUNE PARTIE AVEC LE CODE {}"),
    ("NO TICKET OR CODE ON THE CLIPBOARD", "KEIN TICKET ODER CODE IN DER ZWISCHENABLAGE", "PAS DE TICKET NI DE CODE DANS LE PRESSE-PAPIERS"),
    ("PASTE A TICKET OR CODE", "TICKET ODER CODE EINFÜGEN", "COLLER UN TICKET OU UN CODE"),
    ("TICKET COPIED: SEND IT", "TICKET KOPIERT: SCHICK ES", "TICKET COPIÉ : ENVOIE-LE"),
    ("SEARCHING THE NETWORK ...", "SUCHE IM NETZWERK ...", "RECHERCHE SUR LE RÉSEAU ..."),
    ("NONE FOUND? TYPE THE HOST'S ADDRESS", "KEINS GEFUNDEN? DIE ADRESSE TIPPEN", "RIEN TROUVÉ? TAPE L'ADRESSE"),
    ("(A FIREWALL MAY HIDE IT)", "(EINE FIREWALL VERSTECKT ES VIELLEICHT)", "(UN PARE-FEU LE CACHE PEUT-ÊTRE)"),
    ("MENU", "MENÜ", "MENU"),
    ("(OTHER BUILD)", "(ANDERER STAND)", "(AUTRE VERSION)"),
    ("NOBODY COMES? LET UDP {} IN:", "KOMMT NIEMAND? UDP {} ERLAUBEN:", "PERSONNE? OUVRE UDP {}:"),
    ("SUDO UFW ALLOW {}/UDP", "SUDO UFW ALLOW {}/UDP", "SUDO UFW ALLOW {}/UDP"),
    ("NOBODY COMES? LET VOXCADE IN (FIREWALL)", "KOMMT NIEMAND? VOXCADE IN DER FIREWALL", "PERSONNE? AUTORISE VOXCADE (PARE-FEU)"),
    ("AND MAKE THE NETWORK PRIVATE", "ERLAUBEN, DAS NETZ PRIVAT", "ET RENDS LE RÉSEAU PRIVÉ"),
    ("NOBODY COMES? START VOXCADE", "KOMMT NIEMAND? VOXCADE IM", "PERSONNE? LANCE VOXCADE"),
    ("FROM THE TERMINAL", "TERMINAL STARTEN", "DEPUIS LE TERMINAL"),
];

/// Every word the engine shows, in each of its languages (for the font's
/// test).
pub fn words() -> impl Iterator<Item = &'static str> {
    WORDS.iter().flat_map(|w| [w.0, w.1, w.2])
}

/// The engine's word `en` in `lang` (English where it has no other).
pub fn say(lang: &str, en: &'static str) -> &'static str {
    let column = match lang {
        "de" => 1,
        "fr" => 2,
        _ => return en,
    };
    WORDS.iter().find(|w| w.0 == en).map_or(en, |w| if column == 1 { w.1 } else { w.2 })
}

/// A line made at run time (a network game's end, kept as English
/// until shown) in `lang`: the table's if it has it, else as it is.
pub fn say_line(lang: &str, en: &str) -> String {
    let column = match lang {
        "de" => 1,
        "fr" => 2,
        _ => return en.to_string(),
    };
    WORDS.iter().find(|w| w.0 == en).map_or(en, |w| if column == 1 { w.1 } else { w.2 }).to_string()
}

/// `say` with each `{}` filled, in order, by `parts`.
pub fn say_with(lang: &str, en: &'static str, parts: &[&str]) -> String {
    let mut out = String::new();
    let mut parts = parts.iter();
    let mut rest = say(lang, en);
    while let Some(at) = rest.find("{}") {
        out.push_str(&rest[..at]);
        out.push_str(parts.next().copied().unwrap_or(""));
        rest = &rest[at + 2..];
    }
    out + rest
}

/// A place in a table: 1ST, 1., 1ER.
pub fn ordinal(lang: &str, n: usize) -> String {
    match lang {
        "de" => format!("{n}."),
        "fr" if n == 1 => "1ER".to_string(),
        "fr" => format!("{n}E"),
        _ => {
            let suffix = match (n % 10, n % 100) {
                (1, 11) | (2, 12) | (3, 13) => "TH",
                (1, _) => "ST",
                (2, _) => "ND",
                (3, _) => "RD",
                _ => "TH",
            };
            format!("{n}{suffix}")
        }
    }
}

/// A language by its own name, as LANGUAGE shows it.
pub fn name(lang: &str) -> String {
    match lang {
        "de" => "DEUTSCH".into(),
        "en" => "ENGLISH".into(),
        "fr" => "FRANÇAIS".into(),
        "es" => "ESPAÑOL".into(),
        "it" => "ITALIANO".into(),
        "nl" => "NEDERLANDS".into(),
        "pt" => "PORTUGUÊS".into(),
        other => other.to_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every word the engine shows has its German and French, numbers in
    /// their places; another language sees English.
    #[test]
    fn the_engine_speaks_german_and_french() {
        assert_eq!(say("de", "SETTINGS"), "EINSTELLUNGEN");
        assert_eq!(say("fr", "SETTINGS"), "OPTIONS");
        assert_eq!(say("es", "SETTINGS"), "SETTINGS");
        assert_eq!(say_with("de", "{} MIN AGO", &["5"]), "VOR 5 MIN");
        assert_eq!(say_line("de", "THE HOST LEFT"), "DER HOST IST GEGANGEN");
        assert_eq!(say_line("fr", "SOMETHING THE TABLE LACKS"), "SOMETHING THE TABLE LACKS");
        assert_eq!(say_with("fr", "{}: SELECT   {}: BACK", &["A", "B"]), "A: CHOISIR   B: RETOUR");
        assert_eq!((ordinal("en", 2), ordinal("en", 12), ordinal("de", 3), ordinal("fr", 1), ordinal("fr", 4)), ("2ND".into(), "12TH".into(), "3.".into(), "1ER".into(), "4E".into()));
        let longest = WORDS.iter().map(|w| w.0.chars().count()).max().unwrap();
        for (en, de, fr) in WORDS {
            for word in [de, fr] {
                assert!(word.chars().count() <= longest.max(en.chars().count() + 12), "{word}: too long for where {en} stands");
            }
            assert_eq!((en.matches("{}").count(), de.matches("{}").count(), fr.matches("{}").count()), (en.matches("{}").count(), en.matches("{}").count(), en.matches("{}").count()), "{en}: its numbers");
            for word in [de, fr] {
                assert!(word.chars().all(|c| !c.is_lowercase()), "{word}: in capitals");
            }
        }
    }

    /// The files whose words the menus, the HUD's engine lines and the
    /// network's lines draw: every word in capitals written in them is in
    /// the table (so German and French have it) or named in `NOT_WORDS`
    /// with why. A word added to a menu without its German and French
    /// fails here, not silently in English on a player's screen.
    const MENU_FILES: &[(&str, &str)] = &[
        ("menu.rs", include_str!("menu.rs")),
        ("ui.rs", include_str!("ui.rs")),
        ("network.rs", include_str!("network.rs")),
        ("seating.rs", include_str!("seating.rs")),
        ("settings.rs", include_str!("settings.rs")),
        ("app/mod.rs", include_str!("app/mod.rs")),
        ("app/network.rs", include_str!("app/network.rs")),
        ("app/commands.rs", include_str!("app/commands.rs")),
        ("app/draw.rs", include_str!("app/draw.rs")),
        ("app/events.rs", include_str!("app/events.rs")),
        ("app/front.rs", include_str!("app/front.rs")),
        ("app/keys.rs", include_str!("app/keys.rs")),
        ("net/host.rs", include_str!("net/host.rs")),
        ("net/play.rs", include_str!("net/play.rs")),
    ];

    /// Words in capitals in those files that are no words to translate.
    const NOT_WORDS: &[(&str, &str)] = &[
        ("ABCDEFGHIJKLMNOPQRSTUVWXYZ.", "the letters of initials"),
        ("{} {} MS{}", "a machine's name and its round trip"),
        ("{} {} MS", "a machine's name and its round trip"),
        ("VOX", "the high score table's first lines' initials"),
        ("CAD", "initials"),
        ("ACE", "initials"),
        ("SKY", "initials"),
        ("ZAP", "initials"),
        ("MAX", "initials"),
        ("JOY", "initials"),
        ("KIM", "initials"),
        ("ROY", "initials"),
        ("EVA", "initials"),
        ("XDG_CONFIG_HOME", "a variable of the system"),
        ("HOME", "a variable of the system"),
        ("APPDATA", "a variable of the system"),
    ];

    /// The string literals of Rust source `code` (comments, char literals
    /// and lifetimes stepped over; the tests at its end left out).
    fn literals(code: &str) -> Vec<String> {
        let code = code.find("\n#[cfg(test)]\nmod tests").map_or(code, |at| &code[..at]);
        let b = code.as_bytes();
        let (mut out, mut i) = (Vec::new(), 0);
        while i < b.len() {
            match b[i] {
                b'/' if b.get(i + 1) == Some(&b'/') => i = code[i..].find('\n').map_or(b.len(), |n| i + n),
                b'/' if b.get(i + 1) == Some(&b'*') => i = code[i + 2..].find("*/").map_or(b.len(), |n| i + n + 4),
                b'\'' if b.get(i + 1) == Some(&b'\\') => i = code[i + 2..].find('\'').map_or(b.len(), |n| i + n + 3),
                b'\'' if b.get(i + 2) == Some(&b'\'') => i += 3,
                b'\'' => i += code[i + 1..].chars().next().map_or(1, |c| 1 + c.len_utf8()),
                b'r' if b.get(i + 1) == Some(&b'#') || (b.get(i + 1) == Some(&b'"') && !b[i.saturating_sub(1)].is_ascii_alphanumeric()) => {
                    let hashes = b[i + 1..].iter().take_while(|&&c| c == b'#').count();
                    let start = i + 1 + hashes + 1;
                    let end = "\"".to_string() + &"#".repeat(hashes);
                    let close = code[start..].find(&end).map_or(b.len(), |n| start + n);
                    out.push(code[start..close].to_string());
                    i = close + end.len();
                }
                b'"' => {
                    let mut j = i + 1;
                    let mut s = String::new();
                    while j < b.len() && b[j] != b'"' {
                        if b[j] == b'\\' {
                            j += 1;
                            match b.get(j) {
                                Some(b'n') => s.push('\n'),
                                Some(b'\n') => {
                                    while j + 1 < b.len() && b[j + 1].is_ascii_whitespace() {
                                        j += 1;
                                    }
                                }
                                Some(&c) => s.push(c as char),
                                None => {}
                            }
                            j += 1;
                            continue;
                        }
                        let c = code[j..].chars().next().unwrap();
                        s.push(c);
                        j += c.len_utf8();
                    }
                    out.push(s);
                    i = j + 1;
                }
                _ => i += 1,
            }
        }
        out
    }

    #[test]
    fn every_menu_word_has_its_german_and_french() {
        // The scanner itself, on a sample.
        let sample = "let a = 'x'; fn f<'a>() {} // \"NOT ME\"\n/* \"NOR ME\" */ let s = \"SAY ME\"; let r = r#\"AND \"ME\"\"#; let q = '\\'';";
        assert_eq!(literals(sample), ["SAY ME", "AND \"ME\""]);
        let mut missing = Vec::new();
        let mut seen = 0;
        for (file, code) in MENU_FILES {
            for word in literals(code) {
                let capitals = word.as_bytes().windows(2).any(|w| w[0].is_ascii_uppercase() && w[1].is_ascii_uppercase());
                if !capitals || word.chars().any(char::is_lowercase) {
                    continue;
                }
                seen += 1;
                if !WORDS.iter().any(|w| w.0 == word) && !NOT_WORDS.iter().any(|(w, _)| *w == word) {
                    missing.push(format!("{file}: {word:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "words the menus draw with no German and French in WORDS (or name them in NOT_WORDS):\n{}", missing.join("\n"));
        assert!(seen > 150, "the scan found {seen} words: it reads the files wrongly");
        // Every word of the table is still drawn somewhere.
        let all: Vec<String> = MENU_FILES.iter().flat_map(|(_, code)| literals(code)).collect();
        let unused: Vec<&str> = WORDS.iter().map(|w| w.0).filter(|en| !all.iter().any(|l| l == en)).collect();
        assert!(unused.is_empty(), "words in the table no file names: {unused:?}");
    }
}
