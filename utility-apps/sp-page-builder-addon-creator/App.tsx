import { Blocks } from "lucide-react";
import {
  Workbench,
  type Platform,
  type PluginFile,
  type PluginMeta,
  type Settings,
} from "./Workbench";

// SP Page Builder 6 (Pro) addons for Joomla 5 and 6, each packaged as an installable Joomla
// plugin in the `sppagebuilder` group (`plg_sppagebuilder_<element>`). SP Page Builder loads
// the addons in `/plugins/sppagebuilder/<element>/addons/<addon>/` of every enabled plugin of
// that group.

const PREFIX = "plg_sppagebuilder_";

const ucfirst = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/** The vendor namespace: the setting, else the author's letters, else "Custom". */
function vendorOf(settings: Settings): string {
  const pascal = (s: string) =>
    s
      .split(/[^A-Za-z0-9]+/)
      .filter(Boolean)
      .map(ucfirst)
      .join("")
      .replace(/^[0-9]+/, "");
  return pascal(settings.vendor) || pascal(settings.author) || "Custom";
}

/** Text between `<tag>` and `</tag>` in a manifest. */
function tag(xml: string, name: string): string {
  const m = xml.match(new RegExp(`<${name}[^>]*>([\\s\\S]*?)</${name}>`, "i"));
  return m?.[1]?.trim() ?? "";
}

function manifestOf(files: PluginFile[]): PluginFile | undefined {
  return files.find(
    (f) =>
      !f.path.includes("/") &&
      f.path.toLowerCase().endsWith(".xml") &&
      f.content !== null &&
      /<extension[^>]*type=["']plugin["'][^>]*group=["']sppagebuilder["']|<extension[^>]*group=["']sppagebuilder["'][^>]*type=["']plugin["']/i.test(
        f.content,
      ),
  );
}

/** The plugin's element: the `plugin="…"` attribute of its manifest. */
function elementOf(xml: string): string | null {
  return xml.match(/plugin=["']([a-z0-9_]+)["']/i)?.[1]?.toLowerCase() ?? null;
}

/** A language key's text from the plugin's en-GB files (else any language). */
function translate(value: string, files: PluginFile[]): string {
  if (!/^[A-Z0-9_]+$/.test(value)) return value;
  const ini = files
    .filter((f) => f.content !== null && f.path.endsWith(".ini"))
    .sort((a, b) => Number(b.path.includes("en-GB")) - Number(a.path.includes("en-GB")));
  for (const f of ini) {
    const m = f.content?.match(new RegExp(`^${value}\\s*=\\s*"([^"]*)"`, "m"));
    if (m?.[1]) return m[1].replace(/<[^>]+>/g, "");
  }
  return value;
}

const SYSTEM = (
  settings: Settings,
) => `You are an expert Joomla and SP Page Builder developer. You write custom addons for SP Page Builder 6 Pro that run on Joomla 5 and Joomla 6 with PHP 8.1 or newer, each packaged as an installable Joomla plugin.

## The package (one addon per plugin)
Element name E (lower-case letters and digits, e.g. "c10faq"), Pascal name P = E with the first letter upper-case ("C10faq"), vendor V (${vendorOf(settings)}). Paths are relative to the plugin's folder (plg_sppagebuilder_E), which becomes the top folder of the ZIP:

- E.xml — the manifest:
  <extension type="plugin" group="sppagebuilder" method="upgrade">, <name>PLG_SPPAGEBUILDER_E_UPPER</name>, <author>, <creationDate>, <copyright>, <license>GNU General Public License version 2 or later; see LICENSE.txt</license>, <authorEmail>, <authorUrl>, <version>, <description>PLG_SPPAGEBUILDER_E_UPPER_XML_DESCRIPTION</description>, <namespace path="src">V\\Plugin\\Sppagebuilder\\P</namespace>, <scriptfile>script.php</scriptfile>, and
  <files><folder plugin="E">services</folder><folder>src</folder><folder>addons</folder><folder>language</folder></files>.
  Every file and folder the plugin has must be listed in <files> (script.php is installed through <scriptfile>).
- services/provider.php — returns a ServiceProviderInterface that sets PluginInterface::class to new V\\Plugin\\Sppagebuilder\\P\\Extension\\P($container->get(DispatcherInterface::class), (array) PluginHelper::getPlugin('sppagebuilder', 'E')) and calls setApplication(Factory::getApplication()).
- src/Extension/P.php — namespace V\\Plugin\\Sppagebuilder\\P\\Extension; final class P extends Joomla\\CMS\\Plugin\\CMSPlugin with protected $autoloadLanguage = true and no events (SP Page Builder finds the addon folder by itself while the plugin is enabled).
- script.php — returns a ServiceProviderInterface that sets InstallerScriptInterface::class to an anonymous class: preflight checks the PHP version; postflight on 'install' enables the plugin (UPDATE #__extensions SET enabled = 1 WHERE type = 'plugin' AND folder = 'sppagebuilder' AND element = 'E', through Factory::getContainer()->get(DatabaseInterface::class)), so the addon shows up at once; install, update and uninstall return true.
- language/en-GB/plg_sppagebuilder_E.ini and language/en-GB/plg_sppagebuilder_E.sys.ini — PLG_SPPAGEBUILDER_E_UPPER="SP Page Builder - <addon title>" and PLG_SPPAGEBUILDER_E_UPPER_XML_DESCRIPTION="…". Add other languages only when asked.
- addons/E/admin.php — the addon's settings (below).
- addons/E/site.php — the addon's output (below).
- addons/E/assets/css/E.css and addons/E/assets/js/E.js — only when the addon needs CSS or JavaScript.

## admin.php (SP Page Builder 5/6 settings syntax)
defined('_JEXEC') or die; then SpAddonsConfig::addonConfig([...]) with:
- 'type' => 'content' ('repeatable' when the addon's main content is a list), 'addon_name' => 'E' (exactly the addon's folder name), 'title', 'desc', 'category' => '${settings.category.trim() || "Custom"}', 'icon' => an inline <svg viewBox="0 0 32 32"> using fill="currentColor".
- 'settings' => groups: 'group_key' => ['title' => '…', 'fields' => [ 'field_key' => ['type' => …, 'title' => …, 'desc' => …, 'std' => …], … ]]. A group or field may have 'depends' => [['other_field', '=', value]].
- Field types: text, textarea, editor, number, checkbox (std 1 or 0), select ('values' => ['key' => 'Label', …]), radio, buttons, color, advancedcolor, slider (with 'min', 'max', and 'responsive' => true for per-device values), media, link, icon, typography, padding, margin, alignment, headings, boxshadow, gradient, separator, codeeditor, visibility.
- Lists: 'type' => 'repeatable' with 'attr' => ['item_group' => ['title' => '…', 'fields' => [ … ]]] and an 'std' list of example items; a repeatable may hold another repeatable (a list in a list).
- Give every field a sensible 'std' so the addon looks right as soon as it is dropped on a page.

## site.php
use Joomla\\CMS\\Uri\\Uri; defined('_JEXEC') or die; class SppagebuilderAddonP extends SppagebuilderAddons (the name is "SppagebuilderAddon" followed by the addon folder name with its first letter upper-case). Methods:
- render(): returns the HTML. Wrap it in <div class="sppb-addon sppb-addon-E E …">. Read settings from $this->addon->settings with small private helpers (setting($key, $default), flag($key, $default) for checkboxes, a helper for responsive values, which are objects keyed by device — xl, lg, md, sm, xs — so take ->xl or the first value). Escape every user value with htmlspecialchars($v, ENT_QUOTES, 'UTF-8'); editor fields are HTML and are output as they are.
- css(): optional per-instance CSS scoped to '#sppb-addon-' . $this->addon->id (you may use new CSSHelper($addonId) and ->generateStyle($selector, $settings, ['setting_key' => 'css-property'], ['setting_key' => 'px'])).
- stylesheets() / scripts(): return [Uri::root(true) . '/plugins/sppagebuilder/E/addons/E/assets/css/E.css?v=' . VERSION] (a VERSION class constant equal to the manifest's version) for the asset files.
- Leave out getTemplate(): the editor then renders previews with render().
- JavaScript works for every instance on the page (find them by a data attribute), needs no jQuery, and also runs for addons added later in the editor.

## Rules
- Joomla 6 has no Joomla 3 class aliases and no Joomla\\CMS\\Filesystem: use namespaced classes (Joomla\\CMS\\Factory, Joomla\\CMS\\Uri\\Uri, Joomla\\CMS\\Language\\Text, …) and PHP's is_file()/file_exists(); never JFactory, JUri, JText or WordPress functions.
- Start every PHP file with a docblock (@package, @copyright, @license) and the _JEXEC guard (\\defined('_JEXEC') or die; in namespaced files).
- Prefix CSS classes and JS globals with the element name; make output accessible (labels, aria attributes, keyboard support) and responsive.
- Author: ${settings.author.trim() || "(ask nothing; use a neutral placeholder)"}${settings.authorEmail.trim() ? `, ${settings.authorEmail.trim()}` : ""}${settings.authorUrl.trim() ? `, ${settings.authorUrl.trim()}` : ""}.${settings.instructions.trim() ? `\n- The user always wants: ${settings.instructions.trim()}` : ""}

## Your answer
JSON only: {"summary": "…", "files": [{"path": "…", "content": "…"}], "delete": ["…"]}. "summary" says in one to three sentences what you did, in the user's language. "files" holds whole files (never parts or diffs) with paths relative to the plugin's folder (e.g. "addons/E/site.php", not "plg_sppagebuilder_E/…"). "delete" lists files to remove (usually empty).`;

const platform: Platform = {
  title: "SP Page Builder Addon Creator",
  tagline: "Make and edit SP Page Builder 6 addons as installable Joomla 5/6 plugins.",
  Icon: Blocks,
  noun: "addon",
  nounPlural: "addons",
  installsIn: "Joomla",
  slugify: (name) =>
    name
      .toLowerCase()
      .normalize("NFKD")
      .replace(/[^a-z0-9]/g, "")
      .replace(/^[0-9]+/, "")
      .slice(0, 40),
  slugProblem: (slug) =>
    /^[a-z][a-z0-9]{1,39}$/.test(slug)
      ? null
      : "Use 2–40 lower-case letters and digits, starting with a letter.",
  folderName: (slug) => `${PREFIX}${slug}`,
  metaFiles: (paths) =>
    paths.filter(
      (p) =>
        (!p.includes("/") && p.toLowerCase().endsWith(".xml")) ||
        /^language\/en-GB\/[^/]+\.sys\.ini$/.test(p),
    ),
  describe(folderName, files): PluginMeta {
    const manifest = manifestOf(files);
    const xml = manifest?.content ?? "";
    const slug =
      elementOf(xml) ??
      (folderName.startsWith(PREFIX) ? folderName.slice(PREFIX.length) : folderName);
    const rawName = tag(xml, "name");
    const name = translate(rawName, files).replace(/^SP Page Builder\s*[-–]\s*/i, "");
    return {
      name: name || folderName,
      version: tag(xml, "version"),
      description: translate(tag(xml, "description"), files),
      slug,
      recognized: manifest !== undefined,
    };
  },
  check(meta, files) {
    const problems: string[] = [];
    const has = (p: string) => files.some((f) => f.path === p || f.path.startsWith(`${p}/`));
    const manifest = manifestOf(files);
    if (!manifest?.content) {
      return [
        'No manifest: an .xml file at the top of the folder with <extension type="plugin" group="sppagebuilder">.',
      ];
    }
    const xml = manifest.content;
    if (!tag(xml, "version")) problems.push(`${manifest.path} has no <version>.`);
    const filesBlock = tag(xml, "files");
    for (const m of filesBlock.matchAll(/<(folder|filename)[^>]*>([^<]+)<\/\1>/gi)) {
      const p = m[2]?.trim() ?? "";
      if (p && !has(p)) problems.push(`${manifest.path} lists ${p}, which isn't in the folder.`);
    }
    const script = tag(xml, "scriptfile");
    if (script && !has(script))
      problems.push(`${manifest.path} names ${script} as its script file, which is missing.`);
    if (/<namespace/i.test(xml) && !has("services/provider.php")) {
      problems.push("The manifest declares a namespace but services/provider.php is missing.");
    }
    if (!elementOf(xml)) problems.push('No plugin="…" attribute in the manifest\'s <files>.');
    const addons = [
      ...new Set(
        files
          .map((f) => f.path.match(/^addons\/([^/]+)\//)?.[1])
          .filter((x): x is string => Boolean(x)),
      ),
    ];
    if (addons.length === 0)
      problems.push("No addon: addons/<name>/admin.php and site.php are missing.");
    for (const a of addons) {
      const admin = files.find((f) => f.path === `addons/${a}/admin.php`);
      const site = files.find((f) => f.path === `addons/${a}/site.php`);
      if (!admin) problems.push(`addons/${a}/admin.php is missing.`);
      if (!site) problems.push(`addons/${a}/site.php is missing.`);
      if (
        admin?.content &&
        !new RegExp(`['"]addon_name['"]\\s*=>\\s*['"]${a}['"]`).test(admin.content)
      ) {
        problems.push(`addons/${a}/admin.php: 'addon_name' should be '${a}' (the folder's name).`);
      }
      const cls = `SppagebuilderAddon${ucfirst(a)}`;
      if (site?.content && !new RegExp(`class\\s+${cls}\\b`).test(site.content)) {
        problems.push(`addons/${a}/site.php: the class should be named ${cls}.`);
      }
    }
    for (const f of files) {
      if (!f.content || !f.path.endsWith(".php")) continue;
      if (/\bJ(Factory|Uri|Text|Html|Route|Language)::/.test(f.content)) {
        problems.push(
          `${f.path} uses Joomla 3 names (JFactory, JUri, JText …) that Joomla 6 doesn't have.`,
        );
      }
      if (/Joomla\\CMS\\Filesystem\\/.test(f.content)) {
        problems.push(`${f.path} uses Joomla\\CMS\\Filesystem, which Joomla 6 removed.`);
      }
    }
    if (meta.slug && !/^[a-z0-9_]+$/.test(meta.slug))
      problems.push(`The element “${meta.slug}” should be lower-case letters and digits.`);
    return problems;
  },
  slugForms: (slug) => [slug, ucfirst(slug), slug.toUpperCase()],
  system: SYSTEM,
  newPrompt: ({ name, slug, request, settings }) => `Create a new SP Page Builder addon plugin.

Addon title: ${name}
Element name (E): ${slug}
Pascal name (P): ${ucfirst(slug)}
Vendor namespace (V): ${vendorOf(settings)}
Plugin folder: ${PREFIX}${slug}
Version: 1.0.0
Date: ${new Date().toISOString().slice(0, 10)}

What the addon should do and look like:
${request || "(see the screenshots)"}

Return every file of the plugin.`,
  changePrompt: ({
    meta,
    files,
    request,
  }) => `Change this SP Page Builder addon plugin (element "${meta.slug}", version ${meta.version || "?"}).

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

Return only the files you add or change, each complete. Keep everything else as it is, keep the element name and class names, and keep the manifest's <files> list in step with the files. Don't change the version unless asked.`,
  zipRoot: (_meta, folderName) => folderName,
  zipName: (meta, folderName) => `${folderName}-v${meta.version || "1.0.0"}.zip`,
  importFolder(top, files) {
    const manifest = manifestOf(files);
    const element = manifest?.content ? elementOf(manifest.content) : null;
    if (element) return `${PREFIX}${element}`;
    if (top && top.startsWith(PREFIX) && files.some((f) => f.path.startsWith("addons/")))
      return top;
    return null;
  },
  ideas: [
    "A pricing table with 1–4 plans from a repeatable list: name, price, period, features, a highlighted plan and a button; colors and typography in a Style group.",
    "An FAQ accordion with categories, live search and FAQPage schema.",
    "A before/after image slider with a draggable handle and labels.",
    "A team grid with photo, name, role and social links, 1–4 columns per device.",
  ],
  categoryLabel: "Addon category",
  categoryHelp: "Where new addons are listed in SP Page Builder (default: Custom).",
  vendorLabel: "PHP vendor namespace",
};

export default function App() {
  return <Workbench platform={platform} />;
}
