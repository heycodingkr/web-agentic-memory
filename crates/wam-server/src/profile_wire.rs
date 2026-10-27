//! Converts untrusted JSON profile documents into validated core values and back.

use wam_contracts::{
    BioBlock as WireBlock, BioInline as WireInline, ProfilePutRequest, ProfileResponse,
    SCHEMA_VERSION, ServiceLink as WireServiceLink,
};
use wam_core::profile::{
    BioBlock, BioDocument, BioInline, CreatorProfile, ExternalUrl, ProfileError, PublicHandle,
    ServiceLink,
};

pub fn validate_profile(input: ProfilePutRequest) -> Result<(CreatorProfile, bool), ProfileError> {
    let handle = PublicHandle::parse(&input.handle)?;
    let mut blocks = Vec::with_capacity(input.bio.len());
    for block in input.bio {
        blocks.push(match block {
            WireBlock::Paragraph { content } => BioBlock::Paragraph(convert_inlines(content)?),
            WireBlock::Heading { level, content } => BioBlock::Heading {
                level,
                content: convert_inlines(content)?,
            },
            WireBlock::BulletList { items } => BioBlock::BulletList(convert_items(items)?),
            WireBlock::OrderedList { items } => BioBlock::OrderedList(convert_items(items)?),
        });
    }
    let bio = BioDocument::try_new(blocks)?;
    let mut links = Vec::with_capacity(input.service_links.len());
    for link in input.service_links {
        let url = ExternalUrl::parse(&link.url)?;
        links.push(ServiceLink::new(link.label, url)?);
    }
    let profile = CreatorProfile::try_new(handle, bio, links)?;
    Ok((profile, input.public_opt_in))
}

fn convert_items(items: Vec<Vec<WireInline>>) -> Result<Vec<Vec<BioInline>>, ProfileError> {
    items.into_iter().map(convert_inlines).collect()
}

fn convert_inlines(items: Vec<WireInline>) -> Result<Vec<BioInline>, ProfileError> {
    items
        .into_iter()
        .map(|item| {
            Ok(match item {
                WireInline::Text { text } => BioInline::Text(text),
                WireInline::Strong { text } => BioInline::Strong(text),
                WireInline::Emphasis { text } => BioInline::Emphasis(text),
                WireInline::Link { text, url } => BioInline::Link {
                    text,
                    url: ExternalUrl::parse(&url)?,
                },
            })
        })
        .collect()
}

pub fn profile_response(profile: &CreatorProfile, public_opt_in: bool) -> ProfileResponse {
    let bio = profile
        .bio()
        .blocks()
        .iter()
        .map(|block| match block {
            BioBlock::Paragraph(content) => WireBlock::Paragraph {
                content: wire_inlines(content),
            },
            BioBlock::Heading { level, content } => WireBlock::Heading {
                level: *level,
                content: wire_inlines(content),
            },
            BioBlock::BulletList(items) => WireBlock::BulletList {
                items: items.iter().map(|items| wire_inlines(items)).collect(),
            },
            BioBlock::OrderedList(items) => WireBlock::OrderedList {
                items: items.iter().map(|items| wire_inlines(items)).collect(),
            },
        })
        .collect();
    let service_links = profile
        .links()
        .iter()
        .map(|link| WireServiceLink {
            label: link.label().to_owned(),
            url: link.url().as_str().to_owned(),
        })
        .collect();
    ProfileResponse {
        schema_version: SCHEMA_VERSION,
        handle: profile.handle().as_str().to_owned(),
        bio,
        service_links,
        public_opt_in,
    }
}

fn wire_inlines(items: &[BioInline]) -> Vec<WireInline> {
    items
        .iter()
        .map(|item| match item {
            BioInline::Text(text) => WireInline::Text { text: text.clone() },
            BioInline::Strong(text) => WireInline::Strong { text: text.clone() },
            BioInline::Emphasis(text) => WireInline::Emphasis { text: text.clone() },
            BioInline::Link { text, url } => WireInline::Link {
                text: text.clone(),
                url: url.as_str().to_owned(),
            },
        })
        .collect()
}
