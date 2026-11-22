import { LayoutTemplate } from "lucide-react";
import {
  Workbench,
  type Platform,
  type PluginFile,
  type PluginMeta,
  type Settings,
} from "./Workbench";

// Elementor widgets, each packaged as an installable WordPress plugin (`<slug>/<slug>.php`).
// The widgets are classic widgets (`\Elementor\Widget_Base`), which Elementor 3 and Elementor 4
// (beside its atomic elements) both run.

const SLUG = /^[a-z][a-z0-9]*(-[a-z0-9]+)*$/;

const words = (slug: string) => slug.split("-").filter(Boolean);
const cap = (w: string) => w.charAt(0).toUpperCase() + w.slice(1);
const snake = (slug: string) => words(slug).join("_");
const pascal = (slug: string) => words(slug).map(cap).join("");
const pascalSnake = (slug: string) => words(slug).map(cap).join("_");

/** A plugin header field (`Plugin Name: …`) in a PHP file's first comment. */
function header(php: string, field: string): string {
  const head = php.slice(0, 8000);
  const m = head.match(new RegExp(`^[\\s*#/]*${field}\\s*:\\s*(.+)$`, "im"));
  return m?.[1]?.trim() ?? "";
}

/** The plugin's main file: a top-level PHP file with a `Plugin Name:` header. */
function mainFile(files: PluginFile[]): PluginFile | undefined {
  return files.find(
    (f) =>
      !f.path.includes("/") &&
      f.path.endsWith(".php") &&
      f.content !== null &&
      header(f.content, "Plugin Name") !== "",
  );
}

const SYSTEM = (
  settings: Settings,
) => `You are an expert WordPress and Elementor developer. You write custom Elementor widgets, each packaged as an installable WordPress plugin, for WordPress 6.5 or newer, PHP 7.4 or newer, and Elementor 3.20 up to the current Elementor 4.x (also with Elementor Pro, but without needing it).

Widgets are classic Elementor widgets: a class extending \\Elementor\\Widget_Base. Elementor 4's atomic elements are a different system; don't build those.

## The package (one widget per plugin)
Slug S in kebab-case (e.g. "c10-faq"). Derive every identifier from it, always in these forms, so the plugin can be copied under another slug: S (text domain, handles, CSS prefix), S with "_" for "-" (widget name, function prefix, option names), the same in upper case (constants), Pascal case with "_" between words (classes, e.g. C10_Faq_Widget) and Pascal case without "_" (the PHP namespace, e.g. C10Faq). Paths are relative to the plugin's folder (S), which becomes the top folder of the ZIP:

- S.php — the main file. A plugin header: Plugin Name, Description, Version, Author, Author URI, Text Domain: S, Domain Path: /languages, License: GPL-2.0-or-later, Requires at least: 6.5, Requires PHP: 7.4, Requires Plugins: elementor, Elementor tested up to: 4.4.0, Elementor Pro tested up to: 4.4.0. Then: if ( ! defined( 'ABSPATH' ) ) exit; version and path constants; on plugins_loaded: load the text domain, and if ! did_action( 'elementor/loaded' ) show an admin notice that Elementor is required and stop; else check the minimum Elementor version (ELEMENTOR_VERSION) and PHP version with notices; then hook elementor/widgets/register (require the widget file, $widgets_manager->register( new \\Namespace\\Widget() )), elementor/elements/categories_registered when the widget has its own category ($elements_manager->add_category( slug, [ 'title' => …, 'icon' => 'eicon-…' ] )), and register the assets with wp_register_style / wp_register_script on wp_enqueue_scripts (frontend) — the widget asks for them through get_style_depends() / get_script_depends().
- widgets/class-S-widget.php — the widget: namespace (Pascal) ; class (Pascal_Snake)_Widget extends \\Elementor\\Widget_Base with get_name() (snake), get_title(), get_icon() (an eicon-* class), get_categories() (${settings.category.trim() ? `'${settings.category.trim()}'` : "'general'"} unless asked), get_keywords(), get_style_depends(), get_script_depends(), has_widget_inner_wrapper(): bool returning false (lean markup), is_dynamic_content(): bool returning false unless the output changes per visitor or request, register_controls(), render(), and content_template() for a live editor preview (Backbone/underscore template with {{ }} / {{{ }}} and <# #>; leave it out only when the preview can't be done in JavaScript).
- assets/css/S.css, assets/js/S.js — only when needed.
- languages/ — only when asked for translations; readme.txt (WordPress format) is welcome.

## Controls
- Sections with $this->start_controls_section( 'section_x', [ 'label' => esc_html__( '…', 'S' ), 'tab' => \\Elementor\\Controls_Manager::TAB_CONTENT ] ) … end_controls_section(); a Style tab (TAB_STYLE) for colors, typography, spacing, borders.
- Controls_Manager types: TEXT, TEXTAREA, WYSIWYG, NUMBER, SWITCHER, SELECT, CHOOSE, COLOR, SLIDER, DIMENSIONS, MEDIA, URL, ICONS, REPEATER (with fields from a \\Elementor\\Repeater and a title_field), HEADING, DIVIDER, GALLERY. Per-device values with add_responsive_control.
- Group controls: \\Elementor\\Group_Control_Typography, Group_Control_Border, Group_Control_Box_Shadow, Group_Control_Background, Group_Control_Image_Size.
- Style controls write CSS with 'selectors' => [ '{{WRAPPER}} .S-title' => 'color: {{VALUE}};' ] (sliders: '{{SIZE}}{{UNIT}}'; dimensions: '{{TOP}}{{UNIT}} {{RIGHT}}{{UNIT}} {{BOTTOM}}{{UNIT}} {{LEFT}}{{UNIT}}'), so render() needs no inline styles for them.
- Use global colors and typography as defaults where it fits ('global' => [ 'default' => \\Elementor\\Core\\Kits\\Documents\\Tabs\\Global_Colors::COLOR_PRIMARY ]).
- Give every control a sensible default so the widget looks right as soon as it is dropped on a page.

## render()
$settings = $this->get_settings_for_display(); build attributes with $this->add_render_attribute() / $this->print_render_attribute_string(); escape everything: esc_html(), esc_attr(), esc_url(), wp_kses_post() for WYSIWYG; links with $this->add_link_attributes(); icons with \\Elementor\\Icons_Manager::render_icon(); images with \\Elementor\\Group_Control_Image_Size::get_attachment_image_html().
- JavaScript runs for every instance (find them inside the widget's element) and in the editor too: register a handler with jQuery( window ).on( 'elementor/frontend/init', () => elementorFrontend.hooks.addAction( 'frontend/element_ready/<widget name>.default', ($scope) => … ) ), else run on DOMContentLoaded.

## Rules
- Never use removed or old APIs: no Scheme_Color / Scheme_Typography, no register_widget_type(), no elementor/widgets/widgets_registered, no _register_controls() or _content_template().
- Every PHP file starts with the ABSPATH guard; all strings are translatable with the text domain S; CSS classes are prefixed with S; output is accessible and responsive.
- Author: ${settings.author.trim() || "(use a neutral placeholder)"}${settings.authorEmail.trim() ? `, ${settings.authorEmail.trim()}` : ""}${settings.authorUrl.trim() ? `, ${settings.authorUrl.trim()}` : ""}.${settings.instructions.trim() ? `\n- The user always wants: ${settings.instructions.trim()}` : ""}

## Your answer
JSON only: {"summary": "…", "files": [{"path": "…", "content": "…"}], "delete": ["…"]}. "summary" says in one to three sentences what you did, in the user's language. "files" holds whole files (never parts or diffs) with paths relative to the plugin's folder (e.g. "widgets/class-S-widget.php", not "S/…"). "delete" lists files to remove (usually empty).`;

const platform: Platform = {
  title: "Elementor Widget Creator",
  tagline: "Make and edit Elementor widgets as installable WordPress plugins.",
  Icon: LayoutTemplate,
  noun: "widget",
  nounPlural: "widgets",
  installsIn: "WordPress",
  slugify: (name) =>
    name
      .toLowerCase()
      .normalize("NFKD")
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^[-0-9]+|-+$/g, "")
      .slice(0, 48)
      .replace(/-+$/, ""),
  slugProblem: (slug) =>
    SLUG.test(slug) && slug.length >= 2 && slug.length <= 48
      ? null
      : "Use lower-case letters, digits and single dashes, starting with a letter.",
  folderName: (slug) => slug,
  metaFiles: (paths) => paths.filter((p) => !p.includes("/") && p.endsWith(".php")).slice(0, 8),
  describe(folderName, files): PluginMeta {
    const main = mainFile(files);
    const php = main?.content ?? "";
    const fromMain = main ? main.path.replace(/\.php$/, "") : "";
    const slug = SLUG.test(folderName) ? folderName : fromMain || folderName;
    return {
      name: header(php, "Plugin Name") || folderName,
      version: header(php, "Version"),
      description: header(php, "Description"),
      slug,
      recognized: main !== undefined,
    };
  },
  check(meta, files) {
    const problems: string[] = [];
    const main = mainFile(files);
    if (!main?.content) {
      return ["No main file: a PHP file at the top of the folder with a “Plugin Name:” header."];
    }
    if (!header(main.content, "Version")) problems.push(`${main.path} has no Version header.`);
    if (!/Requires Plugins:\s*.*elementor/i.test(main.content)) {
      problems.push(`${main.path}: add “Requires Plugins: elementor” to the header.`);
    }
    if (
      !/elementor\/widgets\/register['"]/.test(main.content) &&
      !files.some((f) => f.content?.includes("elementor/widgets/register"))
    ) {
      problems.push("No widget is registered on the elementor/widgets/register hook.");
    }
    const php = files.filter((f) => f.content !== null && f.path.endsWith(".php"));
    if (!php.some((f) => /extends\s+\\?(Elementor\\)?Widget_Base\b/.test(f.content ?? ""))) {
      problems.push("No class extends \\Elementor\\Widget_Base.");
    }
    for (const f of php) {
      const c = f.content ?? "";
      if (!/defined\s*\(\s*['"]ABSPATH['"]\s*\)/.test(c)) {
        problems.push(`${f.path} has no ABSPATH guard.`);
      }
      if (
        /register_widget_type\s*\(|widgets_registered|Scheme_(Color|Typography)|function\s+_register_controls|function\s+_content_template/.test(
          c,
        )
      ) {
        problems.push(
          `${f.path} uses Elementor APIs that were removed (register_widget_type, schemes, _register_controls …).`,
        );
      }
    }
    if (main.path !== `${meta.slug}.php`) {
      problems.push(`The main file is usually named after the folder: ${meta.slug}.php.`);
    }
    return problems;
  },
  slugForms: (slug) => [
    slug,
    snake(slug),
    snake(slug).toUpperCase(),
    pascalSnake(slug),
    pascal(slug),
  ],
  system: SYSTEM,
  newPrompt: ({ name, slug, request }) => `Create a new Elementor widget plugin.

Widget title: ${name}
Slug (S): ${slug}
Widget name / prefix: ${snake(slug)}
Constants prefix: ${snake(slug).toUpperCase()}
Class name: ${pascalSnake(slug)}_Widget
Namespace: ${pascal(slug)}
Plugin folder: ${slug}
Version: 1.0.0

What the widget should do and look like:
${request || "(see the screenshots)"}

Return every file of the plugin.`,
  changePrompt: ({
    meta,
    files,
    request,
  }) => `Change this Elementor widget plugin (slug "${meta.slug}", version ${meta.version || "?"}).

The plugin's files now:
${files
  .map((f) =>
    f.content === null
      ? `--- FILE: ${f.path} (binary or too large; kept as it is) ---`
      : `--- FILE: ${f.path} ---\n${f.content}`,
  )
  .join("\n\n")}

The change:
${request || "(see the screenshots)"}

Return only the files you add or change, each complete. Keep everything else as it is, keep the slug, widget name and class names, and don't change the version unless asked.`,
  zipRoot: (meta) => meta.slug,
  zipName: (meta) => `${meta.slug}-${meta.version || "1.0.0"}.zip`,
  importFolder(top, files) {
    const main = mainFile(files);
    if (!main) return null;
    if (top && SLUG.test(top)) return top;
    const fromMain = main.path.replace(/\.php$/, "").toLowerCase();
    return SLUG.test(fromMain) ? fromMain : null;
  },
  ideas: [
    "A pricing table with 1–4 plans from a repeater: name, price, period, features, a highlighted plan and a button; a Style tab with colors, typography and spacing.",
    "An FAQ accordion with live search and FAQPage schema.",
    "A before/after image comparison slider with a draggable handle.",
    "A team grid with photo, name, role and social icons, 1–4 columns per device.",
  ],
  categoryLabel: "Widget category",
  categoryHelp: "The Elementor panel category for new widgets (default: general).",
  vendorLabel: null,
};

export default function App() {
  return <Workbench platform={platform} />;
}
