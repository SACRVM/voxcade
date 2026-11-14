//! A Lua game's state as `voxcade inspect` prints it: the things, the
//! states, the level, the players.

use super::*;

impl<K: Kit> LuaGame<K> {
    /// `Game::inspect`: the game's state as JSON.
    pub(super) fn inspected(&self) -> serde_json::Value {
        let Captured { state, player, level, kept, data, seats } = self.capture().unwrap_or_else(|e| {
            self.halt(e);
            Captured::default()
        });
        let host = &self.stage.host;
        // The level's things; the players' figures are the players'.
        let figures: Vec<Id> = (1..=self.stage.seats.len()).filter_map(|n| self.stage.figure(n)).collect();
        let things: Vec<serde_json::Value> = host
            .ids()
            .into_iter()
            .filter(|id| !figures.contains(id))
            .filter_map(|id| {
                let t = host.get(id)?;
                let p = t.body.pos;
                let mut own = data.iter().find(|(i, _)| *i == id.raw()).map(|(_, d)| value::to_json(d)).filter(|j| j.is_object()).unwrap_or_else(|| serde_json::json!({}));
                if self.stage.kinds[t.kind].has_hp {
                    own["hp"] = t.hp.into();
                }
                if let Some(b) = self.stage.here.lights.get(&id.raw()) {
                    own["light"] = (*b).into();
                }
                let mut thing = serde_json::json!({ "id": id.raw(), "kind": self.stage.kinds[t.kind].name, "x": p.x, "y": p.y, "z": p.z, "state": own });
                if let Some(name) = self.stage.here.names.get(&id.raw()) {
                    thing["name"] = name.clone().into();
                }
                if let Some(model) = self.stage.here.pieces.get(&id.raw()).and_then(|p| p.model.clone()) {
                    thing["model"] = model.into();
                }
                Some(thing)
            })
            .collect();
        let mut player = player.as_ref().map(value::to_json).filter(|j| j.is_object()).unwrap_or_else(|| serde_json::json!({}));
        for key in host.player_keys() {
            if let Some(n) = host.player_number(key) {
                player[*key] = n.into();
            }
        }
        let mut player = serde_json::json!({ "state": player });
        if let Some(t) = host.player_seen() {
            let p = t.body.pos;
            (player["x"], player["y"], player["z"]) = (p.x.into(), p.y.into(), p.z.into());
            player["kind"] = self.stage.kinds.get(t.kind).map(|k| k.name.clone()).into();
        }
        let stage = &self.stage;
        let at = serde_json::json!({ "name": stage.levels.list[stage.level].name, "number": stage.level + 1, "visits": stage.visits[stage.level], "levels": stage.levels.names() });
        let kept: serde_json::Map<String, serde_json::Value> = kept.iter().map(|(n, v)| (n.clone(), value::to_json(v))).collect();
        let mut out = serde_json::json!({ "things": things, "state": state.as_ref().map(value::to_json), "level": level.as_ref().map(value::to_json), "at": at, "kept": kept, "player": player });
        // The other seats, a game of several's.
        if stage.seats.len() > 1 {
            let players: Vec<serde_json::Value> = (2..=stage.seats.len())
                .map(|n| {
                    let s = &stage.seats[n - 1];
                    let mut state = seats.iter().find(|(m, _)| *m == n).map(|(_, v)| value::to_json(v)).filter(|j| j.is_object()).unwrap_or_else(|| serde_json::json!({}));
                    state["score"] = s.score.into();
                    let mut p = serde_json::json!({ "seat": n, "taken": s.taken, "out": s.out, "state": state });
                    if let Some(t) = stage.seen(host::seat_handle(n)) {
                        let at = t.body.pos;
                        (p["x"], p["y"], p["z"]) = (at.x.into(), at.y.into(), at.z.into());
                        p["kind"] = stage.kinds.get(t.kind).map(|k| k.name.clone()).into();
                        p["state"]["hp"] = t.hp.into();
                    }
                    p
                })
                .collect();
            out["players"] = players.into();
        }
        out
    }
}
