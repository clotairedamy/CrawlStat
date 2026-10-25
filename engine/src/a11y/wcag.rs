//! The WCAG success criteria at levels A and AA, across 2.0, 2.1 and 2.2.
//!
//! The wording here is a plain-English summary, not the normative text — the
//! normative text is one click away through [`Criterion::understanding_url`].
//! The test steps are what a person with a browser and a keyboard can do in a
//! few minutes; they are the part of an audit no crawler can do.

/// A WCAG release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WcagVersion {
    V20,
    V21,
    V22,
}

impl WcagVersion {
    pub fn label(self) -> &'static str {
        match self {
            WcagVersion::V20 => "2.0",
            WcagVersion::V21 => "2.1",
            WcagVersion::V22 => "2.2",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    A,
    AA,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::A => "A",
            Level::AA => "AA",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Principle {
    Perceivable,
    Operable,
    Understandable,
    Robust,
}

impl Principle {
    pub const ALL: [Principle; 4] = [
        Principle::Perceivable,
        Principle::Operable,
        Principle::Understandable,
        Principle::Robust,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Principle::Perceivable => "Perceivable",
            Principle::Operable => "Operable",
            Principle::Understandable => "Understandable",
            Principle::Robust => "Robust",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            Principle::Perceivable => "People can see or hear the content, whatever their senses.",
            Principle::Operable => "People can use every control, whatever they use to control it.",
            Principle::Understandable => "People can understand the content and how the interface behaves.",
            Principle::Robust => "Assistive technology can read the page reliably.",
        }
    }
}

/// When a criterion can apply at all. A site with no video has nothing to
/// caption, and saying so is part of a correct report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applies {
    Always,
    /// Prerecorded or live audio or video.
    Media,
    /// Forms and other input.
    Forms,
    /// Logging in or otherwise proving who you are.
    Authentication,
}

#[derive(Debug, Clone, Copy)]
pub struct Criterion {
    /// "1.4.3".
    pub id: &'static str,
    pub name: &'static str,
    pub level: Level,
    pub since: WcagVersion,
    /// 4.1.1 Parsing was removed in WCAG 2.2.
    pub removed_in: Option<WcagVersion>,
    pub principle: Principle,
    pub summary: &'static str,
    pub how_to_test: &'static str,
    pub applies: Applies,
    slug: &'static str,
}

impl Criterion {
    /// W3C's "Understanding" page for the criterion.
    pub fn understanding_url(&self) -> String {
        let edition = match self.removed_in {
            Some(_) => "WCAG21",
            None => "WCAG22",
        };
        format!(
            "https://www.w3.org/WAI/{edition}/Understanding/{}.html",
            self.slug
        )
    }

    /// Is this criterion part of `version` at all?
    pub fn in_version(&self, version: WcagVersion) -> bool {
        self.since <= version && self.removed_in.is_none_or(|removed| removed > version)
    }
}

/// Look up a criterion by number.
pub fn criterion(id: &str) -> Option<&'static Criterion> {
    CRITERIA.iter().find(|criterion| criterion.id == id)
}

use Applies::*;
use Level::*;
use Principle::*;
use WcagVersion::*;

macro_rules! sc {
    ($id:expr, $name:expr, $level:expr, $since:expr, $principle:expr, $applies:expr, $slug:expr,
     $summary:expr, $test:expr) => {
        Criterion {
            id: $id,
            name: $name,
            level: $level,
            since: $since,
            removed_in: None,
            principle: $principle,
            summary: $summary,
            how_to_test: $test,
            applies: $applies,
            slug: $slug,
        }
    };
}

/// Every A and AA success criterion, in numbering order.
pub static CRITERIA: &[Criterion] = &[
    // ─── 1 Perceivable ───────────────────────────────────────────────────
    sc!("1.1.1", "Non-text Content", A, V20, Perceivable, Always, "non-text-content",
        "Images, icons and other non-text content have a text alternative that serves the same purpose. Purely decorative images are hidden from assistive technology.",
        "For each meaningful image, check the alt text says what the image is for, not what it looks like. Check decorative images use alt=\"\". Check icon-only buttons and links have a name. Check CAPTCHAs offer an alternative."),
    sc!("1.2.1", "Audio-only and Video-only (Prerecorded)", A, V20, Perceivable, Media, "audio-only-and-video-only-prerecorded",
        "Recorded audio-only content has a transcript; recorded video with no sound has a transcript or an audio track describing it.",
        "Find every podcast, audio clip and silent video. Check each has a transcript (or for silent video, an audio description) next to it."),
    sc!("1.2.2", "Captions (Prerecorded)", A, V20, Perceivable, Media, "captions-prerecorded",
        "Recorded video with sound has accurate, synchronised captions.",
        "Play each video with captions on. Check captions exist, match the speech, identify speakers and include meaningful sounds. Automatic captions that have not been corrected usually fail."),
    sc!("1.2.3", "Audio Description or Media Alternative (Prerecorded)", A, V20, Perceivable, Media, "audio-description-or-media-alternative-prerecorded",
        "Information shown only visually in recorded video is available through audio description or a full text alternative.",
        "Watch each video with your eyes closed. If you miss important information shown only on screen, check there is an audio-described version or a full text alternative."),
    sc!("1.2.4", "Captions (Live)", AA, V20, Perceivable, Media, "captions-live",
        "Live video with sound (webinars, streams) has real-time captions.",
        "For each live stream or webinar the site hosts, confirm live captioning (human or verified) is provided."),
    sc!("1.2.5", "Audio Description (Prerecorded)", AA, V20, Perceivable, Media, "audio-description-prerecorded",
        "Recorded video has audio description for information shown only visually.",
        "Watch each video with your eyes closed. Check an audio-described version exists when visual-only information is important."),
    sc!("1.3.1", "Info and Relationships", A, V20, Perceivable, Always, "info-and-relationships",
        "Structure that is shown visually — headings, lists, tables, form labels, groups — is also in the markup, so assistive technology can convey it.",
        "With a screen reader or an accessibility inspector, check headings are real headings, lists are lists, data tables have header cells and fields are tied to their labels. Check related radio buttons are grouped (fieldset and legend)."),
    sc!("1.3.2", "Meaningful Sequence", A, V20, Perceivable, Always, "meaningful-sequence",
        "When the order of content matters, the reading order in the code matches it.",
        "Turn off CSS (or use a screen reader's reading mode) and read the page top to bottom. Check it still makes sense and nothing appears out of order."),
    sc!("1.3.3", "Sensory Characteristics", A, V20, Perceivable, Always, "sensory-characteristics",
        "Instructions do not rely only on shape, colour, size, position or sound (\"click the round button on the right\").",
        "Read the instructions on forms and interactive features. Check none depends only on where or what shape something is."),
    sc!("1.3.4", "Orientation", AA, V21, Perceivable, Always, "orientation",
        "Content works in both portrait and landscape, unless one orientation is essential.",
        "On a phone or tablet, rotate the device on key pages. Check nothing is locked to one orientation."),
    sc!("1.3.5", "Identify Input Purpose", AA, V21, Perceivable, Forms, "identify-input-purpose",
        "Fields that collect the user's own details (name, email, address, phone) declare their purpose with the autocomplete attribute.",
        "For each field asking for the user's own information, check it has the matching autocomplete value (for example autocomplete=\"email\")."),
    sc!("1.4.1", "Use of Color", A, V20, Perceivable, Always, "use-of-color",
        "Colour is never the only way information is shown — links in text, errors and chart series need another cue.",
        "Look at the page in greyscale. Check links in paragraphs are still distinguishable (underline or 3:1 contrast plus a hover/focus cue), errors are not shown by red alone, and charts have labels or patterns."),
    sc!("1.4.2", "Audio Control", A, V20, Perceivable, Media, "audio-control",
        "Audio that plays automatically for more than 3 seconds can be paused or muted, independently of the system volume.",
        "Load each page with sound on. If anything plays automatically for more than 3 seconds, check there is a control to stop it near the top of the page."),
    sc!("1.4.3", "Contrast (Minimum)", AA, V20, Perceivable, Always, "contrast-minimum",
        "Text has a contrast ratio of at least 4.5:1 against its background (3:1 for large text: 24px, or 18.5px bold).",
        "Use a contrast checker (browser dev tools, WebAIM, or the Colour Contrast Analyser) on body text, placeholder text, links, buttons, text over images and disabled-looking but active controls."),
    sc!("1.4.4", "Resize Text", AA, V20, Perceivable, Always, "resize-text",
        "Text can be zoomed to 200% without losing content or functionality.",
        "Zoom the browser to 200%. Check no text is cut off, overlapping or hidden, and every feature still works. A viewport that blocks pinch-zoom fails on mobile."),
    sc!("1.4.5", "Images of Text", AA, V20, Perceivable, Always, "images-of-text",
        "Real text is used instead of pictures of text, except for logos or where the presentation is essential.",
        "Look for banners, buttons and headings that are images. Check any text in them is not something that could have been real, styled text."),
    sc!("1.4.10", "Reflow", AA, V21, Perceivable, Always, "reflow",
        "Content reflows to a 320 CSS-pixel-wide viewport without scrolling in two directions (except maps, tables and similar).",
        "Set the browser window to 1280px wide and zoom to 400% (or use a 320px responsive view). Check you only need to scroll vertically and nothing is lost."),
    sc!("1.4.11", "Non-text Contrast", AA, V21, Perceivable, Always, "non-text-contrast",
        "Control boundaries, focus indicators, icons and chart elements needed to understand the page have at least 3:1 contrast.",
        "Check text field borders, checkboxes, toggle states, icon buttons and focus rings against their background with a contrast checker."),
    sc!("1.4.12", "Text Spacing", AA, V21, Perceivable, Always, "text-spacing",
        "Nothing breaks when users increase line height, paragraph, letter and word spacing.",
        "Apply a text-spacing bookmarklet (line height 1.5, paragraph spacing 2×, letter spacing 0.12em, word spacing 0.16em). Check no text is clipped or overlaps."),
    sc!("1.4.13", "Content on Hover or Focus", AA, V21, Perceivable, Always, "content-on-hover-or-focus",
        "Tooltips and pop-ups shown on hover or focus can be dismissed (Esc), can be hovered over, and stay until dismissed.",
        "Trigger each tooltip and hover menu. Check Esc closes it without moving focus, you can move the mouse onto it, and it does not vanish on its own."),
    // ─── 2 Operable ──────────────────────────────────────────────────────
    sc!("2.1.1", "Keyboard", A, V20, Operable, Always, "keyboard",
        "Everything can be done with a keyboard alone.",
        "Put the mouse away. Using Tab, Shift+Tab, Enter, Space, arrows and Esc, complete each key task: navigate, open menus, fill and submit forms, play media, close dialogs."),
    sc!("2.1.2", "No Keyboard Trap", A, V20, Operable, Always, "no-keyboard-trap",
        "Keyboard focus can always be moved away from any component.",
        "Tab through each page including embedded players, maps and dialogs. Check you never get stuck."),
    sc!("2.1.4", "Character Key Shortcuts", A, V21, Operable, Always, "character-key-shortcuts",
        "Single-key shortcuts can be turned off or remapped, or only work when the relevant component has focus.",
        "Find any single-letter keyboard shortcuts (check help pages or try typing letters on the page). Check they can be disabled or remapped."),
    sc!("2.2.1", "Timing Adjustable", A, V20, Operable, Always, "timing-adjustable",
        "Time limits can be turned off, adjusted or extended (with at least 20 seconds' warning).",
        "Look for session timeouts, timed quizzes, auto-advancing carousels and checkout timers. Check users are warned and can extend the time."),
    sc!("2.2.2", "Pause, Stop, Hide", A, V20, Operable, Always, "pause-stop-hide",
        "Anything that moves, blinks or scrolls automatically for more than 5 seconds, or updates automatically, can be paused, stopped or hidden.",
        "Find carousels, animations, tickers and auto-updating feeds. Check each has a visible pause or stop control."),
    sc!("2.3.1", "Three Flashes or Below Threshold", A, V20, Operable, Always, "three-flashes-or-below-threshold",
        "Nothing flashes more than three times a second.",
        "Watch videos and animations. If anything flashes rapidly, test it with a tool such as PEAT."),
    sc!("2.4.1", "Bypass Blocks", A, V20, Operable, Always, "bypass-blocks",
        "There is a way to skip repeated blocks such as the header and navigation: a skip link, landmarks or headings.",
        "Press Tab once on a fresh page. Check a \"Skip to content\" link appears and works, or that the main content is a landmark."),
    sc!("2.4.2", "Page Titled", A, V20, Operable, Always, "page-titled",
        "Every page has a title that describes its topic or purpose.",
        "Check each page's browser-tab title is unique and says what the page is (for single-page apps, check it changes on navigation)."),
    sc!("2.4.3", "Focus Order", A, V20, Operable, Always, "focus-order",
        "Keyboard focus moves in an order that preserves meaning.",
        "Tab through each page. Check focus follows the visual reading order and that opening a dialog moves focus into it, and closing it returns focus."),
    sc!("2.4.4", "Link Purpose (In Context)", A, V20, Operable, Always, "link-purpose-in-context",
        "The purpose of each link is clear from its text, or from its text together with its sentence, paragraph or list item.",
        "List the links (a screen reader's links list helps). Check vague ones like \"Read more\" or \"Click here\" are made clear by their surrounding text or accessible name."),
    sc!("2.4.5", "Multiple Ways", AA, V20, Operable, Always, "multiple-ways",
        "There is more than one way to find a page: navigation plus search, a site map or links between related pages.",
        "Check the site offers at least two of: main navigation, search, site map, A–Z index, or related-page links."),
    sc!("2.4.6", "Headings and Labels", AA, V20, Operable, Always, "headings-and-labels",
        "Headings and labels describe the topic or purpose of what follows.",
        "Read just the headings and field labels. Check each one tells you what the section or field is for."),
    sc!("2.4.7", "Focus Visible", AA, V20, Operable, Always, "focus-visible",
        "Keyboard focus is always visible.",
        "Tab through each page. Check you can always see where focus is — on links, buttons, fields, custom widgets and inside dialogs."),
    sc!("2.4.11", "Focus Not Obscured (Minimum)", AA, V22, Operable, Always, "focus-not-obscured-minimum",
        "A focused element is not entirely hidden by sticky headers, footers, cookie banners or chat widgets.",
        "With the cookie banner and any sticky bars showing, Tab through the page. Check the focused element is never completely covered."),
    sc!("2.5.1", "Pointer Gestures", A, V21, Operable, Always, "pointer-gestures",
        "Anything done with a multi-point or path-based gesture (pinch, swipe) can also be done with a single tap or click.",
        "Find sliders, carousels and maps. Check each swipe or pinch has a button alternative."),
    sc!("2.5.2", "Pointer Cancellation", A, V21, Operable, Always, "pointer-cancellation",
        "Actions happen on release (up-event), so a mis-tap can be abandoned by moving away.",
        "Press and hold on buttons, drag off, then release. Check the action did not fire."),
    sc!("2.5.3", "Label in Name", A, V21, Operable, Always, "label-in-name",
        "A control's accessible name contains the text shown on it, so voice users can say what they see.",
        "Compare visible button and link text with their accessible names (accessibility inspector). Check the visible words are included, ideally at the start."),
    sc!("2.5.4", "Motion Actuation", A, V21, Operable, Always, "motion-actuation",
        "Features triggered by shaking or tilting the device can also be operated with controls, and the motion can be turned off.",
        "On mobile, look for shake-to-undo or tilt features. Check there is an on-screen alternative."),
    sc!("2.5.7", "Dragging Movements", AA, V22, Operable, Always, "dragging-movements",
        "Anything done by dragging can also be done with single clicks or taps.",
        "Find drag-and-drop lists, sliders, map panning and kanban boards. Check each has a click-only alternative (buttons, arrows, a menu)."),
    sc!("2.5.8", "Target Size (Minimum)", AA, V22, Operable, Always, "target-size-minimum",
        "Click and tap targets are at least 24×24 CSS pixels, or spaced so a 24px circle around each does not overlap another.",
        "Measure small icon buttons, pagination, close buttons and inline controls with dev tools. Check each is 24×24px or has enough space around it."),
    // ─── 3 Understandable ────────────────────────────────────────────────
    sc!("3.1.1", "Language of Page", A, V20, Understandable, Always, "language-of-page",
        "The page's main language is declared in the lang attribute, so screen readers pronounce it correctly.",
        "Check the html element has a lang attribute with the right language code (for example lang=\"en\")."),
    sc!("3.1.2", "Language of Parts", AA, V20, Understandable, Always, "language-of-parts",
        "Passages in a different language from the page are marked with their own lang attribute.",
        "Find quotes, names of other-language pages in a language switcher, and foreign phrases. Check each has a correct lang attribute."),
    sc!("3.2.1", "On Focus", A, V20, Understandable, Always, "on-focus",
        "Moving focus onto something never triggers a change of context (navigation, a new window, a submitted form).",
        "Tab through each page. Check nothing navigates, submits or opens a window just because an element received focus."),
    sc!("3.2.2", "On Input", A, V20, Understandable, Forms, "on-input",
        "Changing a setting (selecting an option, ticking a box) does not change context unless the user was told beforehand.",
        "Change each select box, radio and checkbox. Check none navigates or submits on its own without warning."),
    sc!("3.2.3", "Consistent Navigation", AA, V20, Understandable, Always, "consistent-navigation",
        "Navigation repeated across pages appears in the same relative order.",
        "Compare the header, navigation and footer across several page types. Check items appear in the same order."),
    sc!("3.2.4", "Consistent Identification", AA, V20, Understandable, Always, "consistent-identification",
        "Things with the same function are labelled the same way throughout the site.",
        "Check repeated controls (search, download, print, icons) use the same label and icon everywhere."),
    sc!("3.2.6", "Consistent Help", A, V22, Understandable, Always, "consistent-help",
        "Help options (contact details, chat, FAQ link) that appear on several pages are in the same relative place.",
        "Find help and contact mechanisms. Check they appear in the same place on every page that has them."),
    sc!("3.3.1", "Error Identification", A, V20, Understandable, Forms, "error-identification",
        "Input errors are detected and described to the user in text.",
        "Submit each form empty and with invalid values. Check every error is described in text, identifies the field, and is announced by a screen reader."),
    sc!("3.3.2", "Labels or Instructions", A, V20, Understandable, Forms, "labels-or-instructions",
        "Fields have visible labels or instructions, including required fields and expected formats.",
        "Check each field has a visible label (not just placeholder text) and that required fields and formats (dates, phone numbers) are stated."),
    sc!("3.3.3", "Error Suggestion", AA, V20, Understandable, Forms, "error-suggestion",
        "When an error is detected and a fix is known, the user is told how to fix it.",
        "Trigger each validation error. Check the message suggests a correction (\"Enter a date like 31/12/2026\") rather than just \"Invalid\"."),
    sc!("3.3.4", "Error Prevention (Legal, Financial, Data)", AA, V20, Understandable, Forms, "error-prevention-legal-financial-data",
        "Submissions with legal or financial effect, or that change stored data, can be reviewed, corrected, confirmed or reversed.",
        "Go through checkout, bookings, account deletion and legal agreements. Check there is a review step, a confirmation, or a way to undo."),
    sc!("3.3.7", "Redundant Entry", A, V22, Understandable, Forms, "redundant-entry",
        "Users are not asked to re-enter information they already gave in the same process; it is pre-filled or selectable.",
        "Complete multi-step forms (checkout, applications). Check information you already entered — such as the shipping address — is offered again rather than retyped."),
    sc!("3.3.8", "Accessible Authentication (Minimum)", AA, V22, Understandable, Authentication, "accessible-authentication-minimum",
        "Logging in does not require a cognitive test such as remembering or transcribing a password, unless there is an alternative or help (password managers, paste, email links, passkeys).",
        "On the login and sign-up pages, check paste and password managers work in every field, and that any CAPTCHA has a non-puzzle alternative."),
    // ─── 4 Robust ────────────────────────────────────────────────────────
    Criterion {
        id: "4.1.1",
        name: "Parsing",
        level: A,
        since: V20,
        removed_in: Some(V22),
        principle: Robust,
        summary: "Markup has no errors that disrupt assistive technology, such as duplicate ids. Removed in WCAG 2.2 because browsers now repair these; still part of 2.0 and 2.1.",
        how_to_test: "Check for duplicate id values and badly nested elements with a validator. Pay most attention to ids used by labels and ARIA.",
        applies: Always,
        slug: "parsing",
    },
    sc!("4.1.2", "Name, Role, Value", A, V20, Robust, Always, "name-role-value",
        "Every control exposes a name, a role and its current state to assistive technology.",
        "With a screen reader or accessibility inspector, check each custom control (menus, tabs, toggles, accordions, dialogs) announces its name, role and state (expanded, selected, checked), and that state changes are announced."),
    sc!("4.1.3", "Status Messages", AA, V21, Robust, Always, "status-messages",
        "Status messages (\"Added to basket\", \"3 results\", errors after submitting) are announced by screen readers without moving focus.",
        "With a screen reader running, trigger search results, add-to-cart, form submission and loading states. Check each message is announced (role=status, alert or aria-live)."),
];
