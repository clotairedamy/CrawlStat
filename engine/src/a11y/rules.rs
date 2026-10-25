//! What each automated rule means: the criteria it evidences, how much it
//! hurts, who it hurts, and how to fix it.
//!
//! The rule ids themselves live in `parse::accessibility::rule`; this table is
//! kept separate so the parser stays about markup and the report stays about
//! people.

use crate::parse::accessibility::rule;

/// How badly a failure blocks someone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Impact {
    /// Blocks a task outright for some users.
    Critical,
    /// Makes a task very hard.
    Serious,
    /// Makes a task harder or slower.
    Moderate,
    /// An annoyance, or a best practice.
    Minor,
}

impl Impact {
    pub fn as_str(self) -> &'static str {
        match self {
            Impact::Critical => "critical",
            Impact::Serious => "serious",
            Impact::Moderate => "moderate",
            Impact::Minor => "minor",
        }
    }

    pub fn weight(self) -> f64 {
        match self {
            Impact::Critical => 10.0,
            Impact::Serious => 6.0,
            Impact::Moderate => 3.0,
            Impact::Minor => 1.0,
        }
    }
}

/// Whether a hit is a failure on its own or a lead for a person to confirm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Certainty {
    /// The markup alone proves the criterion is not met.
    Fail,
    /// Likely a failure, but there are legitimate exceptions only a person
    /// can rule out.
    Review,
}

impl Certainty {
    pub fn as_str(self) -> &'static str {
        match self {
            Certainty::Fail => "fail",
            Certainty::Review => "review",
        }
    }

    pub fn weight(self) -> f64 {
        match self {
            Certainty::Fail => 1.0,
            Certainty::Review => 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RuleInfo {
    pub id: &'static str,
    pub title: &'static str,
    /// WCAG success criteria this rule gives evidence about.
    pub criteria: &'static [&'static str],
    pub impact: Impact,
    pub certainty: Certainty,
    /// Who meets the barrier.
    pub affects: &'static str,
    pub why: &'static str,
    pub fix: &'static str,
    pub bad: &'static str,
    pub good: &'static str,
}

/// Metadata for a rule id. Every id in `rule::ALL` has an entry; the tests
/// hold that.
pub fn info(id: &str) -> Option<&'static RuleInfo> {
    RULES.iter().find(|info| info.id == id)
}

use Certainty::*;
use Impact::*;

const SCREEN_READER: &str = "Screen reader users";
const KEYBOARD: &str = "Keyboard and switch users";

pub static RULES: &[RuleInfo] = &[
    RuleInfo {
        id: rule::LINK_NO_NAME,
        title: "Links with no accessible name",
        criteria: &["2.4.4", "4.1.2"],
        impact: Critical,
        certainty: Fail,
        affects: "Screen reader and voice control users",
        why: "A screen reader announces just \"link\" — or reads out the URL — so the user cannot tell where it goes. Voice users have nothing to say to click it. Usually an icon link (social icons, logos, a cart) with no text.",
        fix: "Give the link text. For icon links, add visually hidden text or aria-label, and hide the icon itself with aria-hidden=\"true\". For image links, put the destination in the image's alt.",
        bad: "<a href=\"/cart\"><svg>…</svg></a>",
        good: "<a href=\"/cart\" aria-label=\"Basket\"><svg aria-hidden=\"true\">…</svg></a>",
    },
    RuleInfo {
        id: rule::BUTTON_NO_NAME,
        title: "Buttons with no accessible name",
        criteria: &["4.1.2"],
        impact: Critical,
        certainty: Fail,
        affects: "Screen reader and voice control users",
        why: "The button is announced as just \"button\". Close (×), hamburger menu, search and play buttons are the usual culprits — exactly the ones people need.",
        fix: "Put text in the button, or add aria-label describing the action. Mark the icon aria-hidden=\"true\".",
        bad: "<button class=\"close\"><i class=\"icon-x\"></i></button>",
        good: "<button class=\"close\" aria-label=\"Close dialog\"><i class=\"icon-x\" aria-hidden=\"true\"></i></button>",
    },
    RuleInfo {
        id: rule::FIELD_NO_LABEL,
        title: "Form fields with no label",
        criteria: &["1.3.1", "3.3.2", "4.1.2"],
        impact: Critical,
        certainty: Fail,
        affects: "Screen reader, voice control and cognitive-disability users",
        why: "The field is announced as \"edit text\" with no hint of what to type. Placeholder text is not a label: it disappears on typing and is often low contrast.",
        fix: "Add a visible <label for=\"…\"> tied to the field's id, or wrap the field in the label. If a visible label is truly impossible (a lone search box beside a Search button), use aria-label.",
        bad: "<input type=\"email\" placeholder=\"Email\">",
        good: "<label for=\"email\">Email</label>\n<input id=\"email\" type=\"email\" autocomplete=\"email\">",
    },
    RuleInfo {
        id: rule::IMG_NO_ALT,
        title: "Images with no alt attribute",
        criteria: &["1.1.1"],
        impact: Critical,
        certainty: Fail,
        affects: "Blind and low-vision users",
        why: "With no alt at all, screen readers fall back to reading the file name (\"IMG underscore 4032 dot jpeg\"). The image's meaning is lost.",
        fix: "Add alt text that says what the image is for. If the image is decorative, add an empty alt=\"\" so it is skipped.",
        bad: "<img src=\"/team.jpg\">",
        good: "<img src=\"/team.jpg\" alt=\"Our support team at the Lyon office\">\n<img src=\"/divider.svg\" alt=\"\">",
    },
    RuleInfo {
        id: rule::ZOOM_DISABLED,
        title: "Zoom is disabled",
        criteria: &["1.4.4"],
        impact: Critical,
        certainty: Fail,
        affects: "Low-vision users on phones and tablets",
        why: "user-scalable=no or a maximum-scale below 2 stops people pinch-zooming to read. It is one line in the template, so it usually fails every page at once.",
        fix: "Remove user-scalable=no and maximum-scale from the viewport meta tag.",
        bad: "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no\">",
        good: "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
    },
    RuleInfo {
        id: rule::HIDDEN_FOCUSABLE,
        title: "Focusable content hidden from screen readers",
        criteria: &["4.1.2"],
        impact: Serious,
        certainty: Fail,
        affects: SCREEN_READER,
        why: "aria-hidden=\"true\" removes the content from the screen reader, but Tab still lands on the links and buttons inside it. Focus goes somewhere silent. Common with off-screen menus and carousel slides.",
        fix: "When hiding interactive content, also make it unfocusable: use the hidden attribute or display:none, or the inert attribute. Do not put aria-hidden on anything that can take focus.",
        bad: "<nav aria-hidden=\"true\"><a href=\"/\">Home</a></nav>",
        good: "<nav hidden><a href=\"/\">Home</a></nav>\n<!-- or --> <nav inert aria-hidden=\"true\">…</nav>",
    },
    RuleInfo {
        id: rule::BROKEN_ARIA_REF,
        title: "ARIA references to ids that do not exist",
        criteria: &["1.3.1", "4.1.2"],
        impact: Serious,
        certainty: Fail,
        affects: SCREEN_READER,
        why: "aria-labelledby, aria-describedby and aria-controls point at an id that is not on the page, so the label or description is silently lost.",
        fix: "Make the referenced id exist on the same page, or point the attribute at the right id. Template components often generate ids that do not match.",
        bad: "<input aria-labelledby=\"email-label\">  <!-- no element has id=\"email-label\" -->",
        good: "<span id=\"email-label\">Email</span>\n<input aria-labelledby=\"email-label\">",
    },
    RuleInfo {
        id: rule::DUPLICATE_ID_REFERENCED,
        title: "Duplicate ids used by labels or ARIA",
        criteria: &["4.1.2"],
        impact: Serious,
        certainty: Fail,
        affects: SCREEN_READER,
        why: "Two elements share an id that a label or ARIA attribute points to. Only the first one is used, so the second field gets the wrong label or none.",
        fix: "Make every id unique. Components repeated on a page (newsletter forms in header and footer) need a per-instance id.",
        bad: "<label for=\"email\">Email</label><input id=\"email\">\n… <label for=\"email\">Email</label><input id=\"email\">",
        good: "<label for=\"email-header\">Email</label><input id=\"email-header\">\n… <label for=\"email-footer\">Email</label><input id=\"email-footer\">",
    },
    RuleInfo {
        id: rule::PAGE_NO_LANG,
        title: "Page language not declared",
        criteria: &["3.1.1"],
        impact: Serious,
        certainty: Fail,
        affects: "Screen reader users, and anyone using translation or reading tools",
        why: "Without lang, a screen reader reads the page in the user's default voice. French read by an English voice is close to unintelligible.",
        fix: "Add the language code to the html element. It is one change in the base template.",
        bad: "<html>",
        good: "<html lang=\"en\">",
    },
    RuleInfo {
        id: rule::LANG_INVALID,
        title: "Invalid language code",
        criteria: &["3.1.1", "3.1.2"],
        impact: Serious,
        certainty: Fail,
        affects: SCREEN_READER,
        why: "A lang value that is not a real language code (\"english\", \"en_US\", \"uk\" for English) is ignored or picks the wrong voice.",
        fix: "Use a BCP 47 code: a two-letter language, optionally a region with a hyphen (en, en-GB, fr-CA, pt-BR).",
        bad: "<html lang=\"en_US\">",
        good: "<html lang=\"en-US\">",
    },
    RuleInfo {
        id: rule::PAGE_NO_TITLE,
        title: "Page has no title",
        criteria: &["2.4.2"],
        impact: Serious,
        certainty: Fail,
        affects: "Screen reader users and anyone with several tabs open",
        why: "The title is the first thing a screen reader announces and the only thing shown in the tab. Without it people cannot tell pages apart.",
        fix: "Give every page a unique <title> describing it, most specific part first.",
        bad: "<title></title>",
        good: "<title>Delivery and returns – Acme Shop</title>",
    },
    RuleInfo {
        id: rule::IFRAME_NO_TITLE,
        title: "Frames with no title",
        criteria: &["4.1.2"],
        impact: Serious,
        certainty: Fail,
        affects: SCREEN_READER,
        why: "Embedded videos, maps and widgets are announced as just \"frame\", so the user cannot decide whether to enter it.",
        fix: "Add a title attribute describing the embedded content.",
        bad: "<iframe src=\"https://www.youtube.com/embed/…\"></iframe>",
        good: "<iframe src=\"https://www.youtube.com/embed/…\" title=\"Video: setting up your account\"></iframe>",
    },
    RuleInfo {
        id: rule::NESTED_INTERACTIVE,
        title: "Interactive controls inside other controls",
        criteria: &["4.1.2"],
        impact: Serious,
        certainty: Fail,
        affects: "Screen reader and keyboard users",
        why: "A button inside a link (or a link inside a button) has no defined behaviour. Screen readers may announce only the outer control, and the inner one can be unreachable.",
        fix: "Make them siblings. For clickable cards, use one real link and stretch its hit area with CSS instead of wrapping the card in a link.",
        bad: "<a href=\"/p/42\"><div class=\"card\">… <button>Add to basket</button></div></a>",
        good: "<div class=\"card\"><a href=\"/p/42\">Blue mug</a> … <button>Add to basket</button></div>",
    },
    RuleInfo {
        id: rule::ROLE_NOT_FOCUSABLE,
        title: "Custom controls that keyboard users cannot reach",
        criteria: &["2.1.1"],
        impact: Serious,
        certainty: Fail,
        affects: KEYBOARD,
        why: "An element with role=\"button\", \"tab\", \"checkbox\" or similar promises a control, but without tabindex it can never receive keyboard focus.",
        fix: "Use the native element (<button>, <input type=\"checkbox\">). If you must keep the div, add tabindex=\"0\" and handle Enter and Space.",
        bad: "<div role=\"button\" onclick=\"save()\">Save</div>",
        good: "<button type=\"button\" onclick=\"save()\">Save</button>",
    },
    RuleInfo {
        id: rule::INVALID_ROLE,
        title: "Invalid ARIA roles",
        criteria: &["4.1.2"],
        impact: Serious,
        certainty: Fail,
        affects: SCREEN_READER,
        why: "A role that does not exist in WAI-ARIA is ignored, so the element is exposed as something other than what the design intended.",
        fix: "Use a role from the WAI-ARIA specification, or remove the attribute and use the right HTML element.",
        bad: "<div role=\"dropdown\">…</div>",
        good: "<div role=\"menu\">…</div>  <!-- or a native <select> -->",
    },
    RuleInfo {
        id: rule::INVALID_AUTOCOMPLETE,
        title: "Invalid autocomplete values",
        criteria: &["1.3.5"],
        impact: Serious,
        certainty: Fail,
        affects: "People with motor or cognitive disabilities who rely on autofill",
        why: "An unrecognised autocomplete value is ignored, so browsers and assistive tools cannot fill or label the field.",
        fix: "Use a token from the HTML autocomplete list: name, given-name, email, tel, street-address, postal-code, cc-number, and so on. Use off only where autofill must be prevented.",
        bad: "<input name=\"mail\" autocomplete=\"e-mail\">",
        good: "<input name=\"mail\" autocomplete=\"email\">",
    },
    RuleInfo {
        id: rule::LIST_STRUCTURE,
        title: "Broken list markup",
        criteria: &["1.3.1"],
        impact: Serious,
        certainty: Fail,
        affects: SCREEN_READER,
        why: "A list containing things other than <li>, or an <li> outside a list, breaks the \"list, 5 items\" announcement and item navigation.",
        fix: "Only put <li> directly inside <ul> and <ol>; wrap other content inside the <li>.",
        bad: "<ul><div><li>One</li></div></ul>",
        good: "<ul><li><div>One</div></li></ul>",
    },
    RuleInfo {
        id: rule::META_REFRESH,
        title: "Timed page refresh or redirect",
        criteria: &["2.2.1"],
        impact: Serious,
        certainty: Fail,
        affects: "Screen reader users, slow readers and people with cognitive disabilities",
        why: "The page reloads or moves after a delay the user did not choose, often mid-sentence.",
        fix: "Redirect on the server (301/302) or with a delay of 0. To refresh content, update it in place and let the user control it.",
        bad: "<meta http-equiv=\"refresh\" content=\"30\">",
        good: "<!-- server-side 301, or an in-page \"Refresh\" button -->",
    },
    RuleInfo {
        id: rule::BLINK_MARQUEE,
        title: "Blinking or scrolling text",
        criteria: &["2.2.2"],
        impact: Serious,
        certainty: Fail,
        affects: "People with attention, reading or vestibular disorders",
        why: "<blink> and <marquee> move content with no way to stop it.",
        fix: "Remove the element. If movement is needed, use CSS animation with a pause control that respects prefers-reduced-motion.",
        bad: "<marquee>Sale ends today!</marquee>",
        good: "<p class=\"banner\">Sale ends today!</p>",
    },
    RuleInfo {
        id: rule::NO_BYPASS,
        title: "No way to skip to the content",
        criteria: &["2.4.1"],
        impact: Serious,
        certainty: Fail,
        affects: KEYBOARD,
        why: "With no <main> landmark, no skip link and no headings, keyboard users must tab through the whole header on every page.",
        fix: "Wrap the main content in <main>, add a \"Skip to content\" link as the first focusable element, and use headings.",
        bad: "<div id=\"content\">…</div>",
        good: "<a href=\"#main\" class=\"skip-link\">Skip to content</a> …\n<main id=\"main\">…</main>",
    },
    RuleInfo {
        id: rule::EMPTY_HEADING,
        title: "Empty headings",
        criteria: &["1.3.1", "2.4.6"],
        impact: Moderate,
        certainty: Fail,
        affects: SCREEN_READER,
        why: "Screen reader users jump between headings to scan a page. An empty heading is a stop with nothing there. Often a heading used for spacing, or an icon with no text.",
        fix: "Put text in the heading, or remove it and use CSS for spacing.",
        bad: "<h2><i class=\"icon-star\"></i></h2>",
        good: "<h2><i class=\"icon-star\" aria-hidden=\"true\"></i> Customer favourites</h2>",
    },
    RuleInfo {
        id: rule::POSITIVE_TABINDEX,
        title: "Positive tabindex",
        criteria: &["2.4.3"],
        impact: Serious,
        certainty: Review,
        affects: KEYBOARD,
        why: "tabindex above 0 moves the element to the front of the tab order, ahead of everything else, so focus jumps around the page.",
        fix: "Use tabindex=\"0\" (or none on native controls) and arrange the DOM in reading order.",
        bad: "<input tabindex=\"3\">",
        good: "<input>",
    },
    RuleInfo {
        id: rule::HEADING_SKIP,
        title: "Skipped heading levels",
        criteria: &["1.3.1"],
        impact: Moderate,
        certainty: Review,
        affects: SCREEN_READER,
        why: "Going from h2 straight to h4 suggests a missing section and makes the outline harder to follow. Not always a failure, but often a sign headings are chosen for their size.",
        fix: "Pick heading levels by structure, not appearance, and style them with CSS.",
        bad: "<h2>Plans</h2> <h4>Starter</h4>",
        good: "<h2>Plans</h2> <h3 class=\"small\">Starter</h3>",
    },
    RuleInfo {
        id: rule::DUPLICATE_ID,
        title: "Duplicate ids",
        criteria: &["4.1.1"],
        impact: Minor,
        certainty: Fail,
        affects: "Mostly a robustness issue",
        why: "Ids should be unique. Duplicates nobody references rarely affect users today, which is why WCAG 2.2 retired 4.1.1 — but they fail 2.0 and 2.1, and break as soon as a label or script points at them.",
        fix: "Make ids unique, or remove ids nothing uses.",
        bad: "<div id=\"promo\">…</div> <div id=\"promo\">…</div>",
        good: "<div id=\"promo-top\">…</div> <div id=\"promo-bottom\">…</div>",
    },
    RuleInfo {
        id: rule::AUTOPLAY_AUDIO,
        title: "Media that plays sound automatically",
        criteria: &["1.4.2"],
        impact: Serious,
        certainty: Review,
        affects: SCREEN_READER,
        why: "Sound that starts by itself talks over the screen reader, making it hard to even find the stop button. Fails if it lasts more than 3 seconds without a control.",
        fix: "Do not autoplay with sound. Add muted, or let the user press play. Provide visible pause and volume controls.",
        bad: "<video src=\"/intro.mp4\" autoplay></video>",
        good: "<video src=\"/intro.mp4\" controls></video>\n<!-- or --> <video src=\"/loop.mp4\" autoplay muted loop></video>",
    },
    RuleInfo {
        id: rule::VIDEO_NO_CAPTIONS,
        title: "Video with no caption track",
        criteria: &["1.2.2"],
        impact: Critical,
        certainty: Review,
        affects: "Deaf and hard-of-hearing users",
        why: "A <video> with sound and no captions or subtitles track. Captions might be burned in or provided by the player, so confirm by watching.",
        fix: "Add a <track kind=\"captions\"> with an accurate WebVTT file. Corrected captions, not raw auto-captions.",
        bad: "<video src=\"/demo.mp4\" controls></video>",
        good: "<video src=\"/demo.mp4\" controls>\n  <track kind=\"captions\" src=\"/demo.en.vtt\" srclang=\"en\" label=\"English\" default>\n</video>",
    },
    RuleInfo {
        id: rule::FOCUS_OUTLINE_REMOVED,
        title: "Focus outline removed",
        criteria: &["2.4.7"],
        impact: Serious,
        certainty: Review,
        affects: "Sighted keyboard users",
        why: "outline:none on :focus with nothing drawn instead leaves keyboard users unable to see where they are. Confirm in a browser: another style may draw a ring.",
        fix: "Remove the outline only for mouse users with :focus:not(:focus-visible), or replace it with a clear visible style.",
        bad: "a:focus { outline: none; }",
        good: ":focus-visible { outline: 3px solid #1a73e8; outline-offset: 2px; }",
    },
    RuleInfo {
        id: rule::CLICK_HANDLER_NOT_FOCUSABLE,
        title: "Clickable elements that are not keyboard accessible",
        criteria: &["2.1.1"],
        impact: Serious,
        certainty: Review,
        affects: KEYBOARD,
        why: "A div or span with onclick and no role or tabindex works with a mouse only. Confirm the same action is not available elsewhere by keyboard.",
        fix: "Use a <button> (for actions) or <a href> (for navigation) instead.",
        bad: "<span onclick=\"openMenu()\">Menu</span>",
        good: "<button type=\"button\" onclick=\"openMenu()\" aria-expanded=\"false\">Menu</button>",
    },
    RuleInfo {
        id: rule::TABLE_NO_HEADERS,
        title: "Data tables with no header cells",
        criteria: &["1.3.1"],
        impact: Moderate,
        certainty: Review,
        affects: SCREEN_READER,
        why: "Without <th>, a screen reader reads cells with no column or row names. If it is a layout table, that is different — confirm which it is.",
        fix: "Mark header cells with <th scope=\"col\"> or scope=\"row\". If the table is only for layout, use CSS instead, or add role=\"presentation\".",
        bad: "<table><tr><td>Plan</td><td>Price</td></tr>…</table>",
        good: "<table><tr><th scope=\"col\">Plan</th><th scope=\"col\">Price</th></tr>…</table>",
    },
    RuleInfo {
        id: rule::LABEL_NOT_IN_NAME,
        title: "Visible label not in accessible name",
        criteria: &["2.5.3"],
        impact: Moderate,
        certainty: Review,
        affects: "Voice control users",
        why: "The button says one thing, but aria-label says another. A voice user saying \"click Buy now\" gets nothing because its name is \"Add product to cart\".",
        fix: "Start the aria-label with the visible text, or remove aria-label and let the visible text be the name.",
        bad: "<button aria-label=\"Add product to cart\">Buy now</button>",
        good: "<button aria-label=\"Buy now: Blue mug\">Buy now</button>",
    },
    RuleInfo {
        id: rule::VAGUE_LINK_TEXT,
        title: "Vague link text",
        criteria: &["2.4.4"],
        impact: Minor,
        certainty: Review,
        affects: "Screen reader users browsing a list of links",
        why: "\"Read more\", \"Click here\" and \"Learn more\" say nothing out of context. WCAG allows them if the surrounding sentence makes the purpose clear, so check.",
        fix: "Make the link text say where it goes, or add hidden context.",
        bad: "<a href=\"/pricing\">Read more</a>",
        good: "<a href=\"/pricing\">Read more <span class=\"visually-hidden\">about pricing</span></a>",
    },
    RuleInfo {
        id: rule::MISSING_AUTOCOMPLETE,
        title: "Personal-data fields without autocomplete",
        criteria: &["1.3.5"],
        impact: Minor,
        certainty: Review,
        affects: "People with motor or cognitive disabilities who rely on autofill",
        why: "Fields that look like they ask for the user's own name, email, phone or address have no autocomplete. Confirm they are about the user (not, say, a gift recipient).",
        fix: "Add the matching autocomplete token.",
        bad: "<input type=\"tel\" name=\"phone\">",
        good: "<input type=\"tel\" name=\"phone\" autocomplete=\"tel\">",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a11y::wcag;

    #[test]
    fn every_rule_has_metadata() {
        for id in rule::ALL {
            assert!(info(id).is_some(), "no metadata for {id}");
        }
        assert_eq!(RULES.len(), rule::ALL.len(), "metadata for an unknown rule");
    }

    #[test]
    fn every_cited_criterion_exists() {
        for info in RULES {
            assert!(!info.criteria.is_empty(), "{} cites nothing", info.id);
            for id in info.criteria {
                assert!(wcag::criterion(id).is_some(), "{} cites unknown {id}", info.id);
            }
        }
    }
}
