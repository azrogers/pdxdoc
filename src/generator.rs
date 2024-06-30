use std::rc::Rc;

use crate::{config::Profile, dossier::Dossier, page::Page};

struct SiteProfile {
    profile: Profile,
    dossier: Rc<Dossier>,
    pages: Vec<Box<dyn Page>>,
}

impl SiteProfile {
    pub fn new(profile: Profile, dossier: Dossier) -> SiteProfile {
        let dossier = Rc::new(dossier);
        let pages = Dossier::create_pages(dossier.clone());

        SiteProfile {
            profile,
            dossier,
            pages,
        }
    }
}

pub struct SiteGenerator {
    profiles: Vec<SiteProfile>,
}

impl SiteGenerator {
    pub fn new() -> SiteGenerator {
        SiteGenerator {
            profiles: Vec::new(),
        }
    }

    pub fn add_profile(&mut self, profile: Profile, dossier: Dossier) {
        self.profiles.push(SiteProfile::new(profile, dossier))
    }
}
