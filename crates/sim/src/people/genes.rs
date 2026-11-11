//! DNA: two alleles per locus (one from each parent), expressed into looks and personality
//! leanings. The loci and colour ramps are data (data/base/genes.ron).

use crate::content::{GeneParams, LocusKind, hex_colour};
use crate::rng::Rng;
use serde::{Deserialize, Serialize};

use super::Sex;

/// Alleles per locus, in the order of `GeneParams::loci`. Each allele is 0 or 1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Genome {
    pub loci: Vec<[u8; 2]>,
}

impl Genome {
    /// A founder: alleles drawn from the population frequencies.
    pub fn random(p: &GeneParams, rng: &mut Rng) -> Genome {
        Genome { loci: p.loci.iter().map(|l| [rng.chance(l.freq) as u8, rng.chance(l.freq) as u8]).collect() }
    }

    /// A child: one allele from each parent at every locus, with rare mutation.
    pub fn inherit(mother: &Genome, father: &Genome, rng: &mut Rng) -> Genome {
        let pick = |g: &[u8; 2], rng: &mut Rng| {
            let a = g[rng.below(2) as usize];
            if rng.chance(0.001) { 1 - a } else { a }
        };
        Genome { loci: mother.loci.iter().zip(&father.loci).map(|(m, f)| [pick(m, rng), pick(f, rng)]).collect() }
    }

    /// Sum of a trait's additive loci, or 1/0 for a recessive trait (shows only with two copies).
    pub fn score(&self, p: &GeneParams, trait_id: &str) -> f32 {
        let mut s = 0.0;
        for (l, a) in p.loci.iter().zip(&self.loci) {
            if l.trait_id != trait_id {
                continue;
            }
            s += match l.kind {
                LocusKind::Additive => (a[0] + a[1]) as f32 * l.effect,
                LocusKind::Recessive => (a[0] == 1 && a[1] == 1) as u8 as f32 * l.effect,
            };
        }
        s
    }

    /// Which parent a trait mostly came from, for the inspector: compares the trait's alleles with
    /// each parent's (None for founders).
    pub fn allele_text(&self, p: &GeneParams, trait_id: &str) -> String {
        p.loci
            .iter()
            .zip(&self.loci)
            .filter(|(l, _)| l.trait_id == trait_id)
            .map(|(_, a)| {
                let c = |x: u8| if x == 1 { 'A' } else { 'a' };
                format!("{}{}", c(a[0]), c(a[1]))
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// How someone looks, from their genes (and a little chance).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Looks {
    pub height: f32,
    /// 0 slight .. 1 broad
    pub build: f32,
    pub skin: [u8; 3],
    pub hair: [u8; 3],
    pub eyes: [u8; 3],
    pub eye_word: String,
    pub hair_word: String,
    pub skin_word: String,
    /// 0 short, 1 long
    pub hair_style: u8,
    pub beard: bool,
    pub clothes: [u8; 3],
}

fn ramp(colours: &[String], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0) * (colours.len() - 1) as f32;
    let i = (t.floor() as usize).min(colours.len() - 2);
    let (a, b) = (hex_colour(&colours[i]), hex_colour(&colours[i + 1]));
    let f = t - i as f32;
    [0, 1, 2].map(|k| (a[k] as f32 + (b[k] as f32 - a[k] as f32) * f).round() as u8)
}

pub fn express(g: &Genome, p: &GeneParams, sex: Sex, clothes: [u8; 3], rng: &mut Rng) -> Looks {
    let base = if sex == Sex::Female { p.base_height.0 } else { p.base_height.1 };
    let height = base + g.score(p, "height") + (rng.next_f32() - 0.5) * 2.0 * p.height_noise;
    let build = (g.score(p, "build") / 1.0 + (rng.next_f32() - 0.5) * 0.2).clamp(0.0, 1.0);
    let skin_t = g.score(p, "skin");
    let dark = g.score(p, "hair_dark");
    let red = g.score(p, "red_hair") > 0.5;
    let (hair, hair_word) = if red && dark < 0.6 {
        (hex_colour(&p.hair_red), "red".to_string())
    } else {
        let w = ["blond", "light brown", "brown", "dark brown", "black"][((dark * 4.0).round() as usize).min(4)];
        (ramp(&p.hair, dark), w.to_string())
    };
    let brown = g.score(p, "eye_brown");
    let green = g.score(p, "eye_green");
    let (eyes, eye_word) = if brown >= 0.5 {
        if brown >= 1.0 { (hex_colour(&p.eyes.brown), "brown") } else { (hex_colour(&p.eyes.hazel), "hazel") }
    } else if green >= 0.5 {
        (hex_colour(&p.eyes.green), "green")
    } else {
        (hex_colour(&p.eyes.blue), "blue")
    };
    let skin_word = ["very fair", "fair", "olive", "brown", "dark brown"][((skin_t * 4.0).round() as usize).min(4)];
    let hair_style = if sex == Sex::Female { (rng.chance(0.8)) as u8 } else { (rng.chance(0.15)) as u8 };
    Looks {
        height,
        build,
        skin: ramp(&p.skin, skin_t),
        hair,
        eyes,
        eye_word: eye_word.to_string(),
        hair_word,
        skin_word: skin_word.to_string(),
        hair_style,
        beard: sex == Sex::Male && rng.chance(0.5),
        clothes,
    }
}

/// Personality leanings (0..1): curiosity, diligence, sociability, warmth, calm. Part genes, part
/// upbringing and chance.
pub fn personality(g: &Genome, p: &GeneParams, rng: &mut Rng) -> [f32; 5] {
    let h = p.personality_heritability;
    ["curiosity", "diligence", "sociability", "warmth", "calm"].map(|t| (g.score(p, t) * h + rng.next_f32() * (1.0 - h)).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Content;

    #[test]
    fn children_take_one_allele_from_each_parent() {
        let c = Content::base();
        let mut r = Rng::new(3);
        let m = Genome::random(&c.genes, &mut r);
        let f = Genome::random(&c.genes, &mut r);
        for _ in 0..50 {
            let k = Genome::inherit(&m, &f, &mut r);
            let mut mutated = 0;
            for i in 0..k.loci.len() {
                mutated += !m.loci[i].contains(&k.loci[i][0]) as u32 + !f.loci[i].contains(&k.loci[i][1]) as u32;
            }
            assert!(mutated <= 1);
        }
    }

    #[test]
    fn two_blue_eyed_parents_have_blue_eyed_children() {
        let c = Content::base();
        let p = &c.genes;
        let mut r = Rng::new(9);
        let blue = |r: &mut Rng| {
            let mut g = Genome::random(p, r);
            for (l, a) in p.loci.iter().zip(g.loci.iter_mut()) {
                if l.trait_id == "eye_brown" || l.trait_id == "eye_green" {
                    *a = [0, 0];
                }
            }
            g
        };
        let (m, f) = (blue(&mut r), blue(&mut r));
        let blues = (0..200)
            .filter(|_| express(&Genome::inherit(&m, &f, &mut r), p, Sex::Female, [0; 3], &mut r).eye_word == "blue")
            .count();
        assert!(blues >= 195, "{blues} of 200 children had blue eyes");
    }
}
