// Things to generate:
// - [...] Docs
// - [ ] Pages
//   - [ ] Downloads
//   - [ ] Regular static pages
// - [ ] Blog
// - [ ] News
//
// Features to add:
// - folder-as-file
// - templating
// - choose css framework
// - serve
// - generate RSS/Atom feeds

use std::{
    cmp::Ordering,
    convert::TryInto,
    io::Write,
    path::{Path, PathBuf},
};

extern crate anyhow;
mod config;
mod document;
mod server;
mod util;

use anyhow::Context;
pub use config::Config;
use structopt::StructOpt;

extern crate serde;

use document::*;

use crate::{
    config::GlobalMeta,
    util::{filename_to_string, write_file_as_folder_with_index},
};

// TODO: Involve templates here for easier modification?
fn generate_docnav_html(root: &document::Category, focused_doc_path: &Path) -> String {
    let mut str = String::new();
    // For now, fully expanded. Will fix later.
    str += &format!(
        "<p><a href=/{} class=\"category-link\">{}</a></p>",
        root.path.display(),
        root.meta.title
    );
    str += "<div class=\"category\">";
    for cat in &root.sub_categories {
        str += &generate_docnav_html(cat, focused_doc_path);
    }
    for doc in &root.documents {
        // TODO: these links don't match docusaurus!
        str += &format!("<a href=/{}>{}</a>", doc.path.display(), doc.meta.title,);
    }
    str += "</div>";

    str
}

fn generate_doctree(
    config: &Config,
    folder: &str,
    handlebars: &mut handlebars::Handlebars,
) -> anyhow::Result<()> {
    // First, build the tree and convert all the markdown to html and metadata.
    let root_folder = config.indir.join(folder);
    let out_root_folder = config.outdir.clone();
    let root_cat = document::Category::from_folder_tree(&root_folder, &config.markdown_options)?;

    // Write out all the docs. Don't need recursion here so we can linearize.
    // Note that we also generate the categories as documents in `all_documents`.
    let docs = root_cat.all_documents(handlebars)?;
    for doc in docs {
        let target_path = out_root_folder.join(doc.path);

        util::create_folder_if_missing(&target_path)?;

        // We apply the template right here.
        let mut context = PageContext::new(Some(doc.meta.title), Some(doc.html));
        context.sidebar = Some(generate_docnav_html(&root_cat, &target_path));
        let html = handlebars.render("doc", &context)?;

        println!("Writing doc {}", target_path.display());
        write_file_as_folder_with_index(&target_path, html, true)?;
    }

    // MD documents get wrapped into our doc template.
    // println!("{:#?}", root_cat);

    Ok(())
}

// Posts should be passed-in in reverse time order.
fn generate_blog_sidebar(
    all_posts: &Vec<Document>,
    handlebars: &mut handlebars::Handlebars,
) -> anyhow::Result<String> {
    let context = SidebarContext {
        links: all_posts
            .iter()
            .map(|doc| doc.to_doclink())
            .collect::<Vec<_>>(),
    };

    let output = handlebars.render("blog_sidebar", &context)?;
    Ok(output)
}
fn generate_blog(
    config: &Config,
    folder: &str,
    handlebars: &mut handlebars::Handlebars,
) -> anyhow::Result<()> {
    // For the blog

    let root_folder = config.indir.join(folder);
    let out_root_folder = config.outdir.join(folder);

    util::create_folder_if_missing(&out_root_folder)?;

    let mut documents = vec![];

    let listing = root_folder.read_dir()?;
    for entry in listing {
        let entry = entry?;
        let file_name = PathBuf::from(entry.file_name());
        let Some(os_str) = file_name.extension() else {
            continue;
        };
        match os_str.to_str().unwrap() {
            "md" => {}
            _ => {
                println!("Skipping file {}", file_name.display());
                continue;
            }
        }
        let name = util::filename_to_string(&entry.file_name());

        let parts: [&str; 4] = name.splitn(4, '-').collect::<Vec<_>>().try_into().unwrap();
        let mut doc = Document::from_md(
            &root_folder.join(entry.file_name()),
            &config.markdown_options,
        )?;

        let [year, month, day, remainder] = parts;
        doc.meta.date = format!("{}-{}-{}", year, month, day);
        if doc.meta.slug.is_empty() {
            println!(
                "Warning: Blog entry missing slug, autodetecting {}: {}",
                name, remainder
            );
            doc.meta.slug = remainder.to_string();
        }
        assert!(!doc.meta.slug.is_empty());
        doc.meta.url = Some(format!("/{folder}/{}", &doc.meta.slug));
        doc.path = out_root_folder.join(&doc.meta.slug);
        documents.push(doc);
    }

    documents.sort_by(|a, b| {
        a.meta
            .date
            .partial_cmp(&b.meta.date)
            .unwrap_or(Ordering::Equal)
            .reverse()
    });

    for doc in &documents {
        let mut context = PageContext::from_document(&doc);

        // First, render the blog post itself, without the surrounding chrome. This is so that we can add on
        // more blog posts underneath later for a more continuous experience.
        let post_html = handlebars.render("blog_post", &context)?;
        let sidebar = generate_blog_sidebar(&documents, handlebars)?;

        // Now, use that as contents and render into a doc template.
        context.contents = Some(post_html);
        context.sidebar = Some(sidebar);
        let html = handlebars.render("doc", &context)?;

        let target_path = &doc.path;
        println!("Writing blog post {}", target_path.display());
        util::write_file_as_folder_with_index(&target_path, html, false)?;
    }

    // Generate the root blog post.
    if let Some(doc) = documents.get(0) {
        let mut context = PageContext::from_document(&doc);

        // First, render the blog post itself, without the surrounding chrome. This is so that we can add on
        // more blog posts underneath later for a more continuous experience.
        let post_html = handlebars.render("blog_post", &context)?;
        let sidebar = generate_blog_sidebar(&documents, handlebars)?;

        // Now, use that as contents and render into a doc template.
        context.contents = Some(post_html);
        context.sidebar = Some(sidebar);
        let html = handlebars.render("doc", &context)?;

        let target_path = out_root_folder;
        println!("Writing blog root {}", target_path.display());
        util::write_file_as_folder_with_index(&target_path, html, false)?;
    }

    Ok(())
}

fn generate_pages(
    config: &Config,
    folder: &str,
    handlebars: &mut handlebars::Handlebars,
) -> anyhow::Result<()> {
    let root_folder = config.indir.join(folder);
    // pages are generated directly into the root.
    let out_root_folder = &config.outdir;

    let listing = root_folder.read_dir()?;
    for entry in listing {
        let entry = entry?;
        let path = root_folder.join(entry.file_name());
        let mut file_name = PathBuf::from(entry.file_name());
        // TODO: Parse metadata out of the files somehow!
        let Some(os_str) = path.extension() else {
            continue;
        };
        println!("considering {}", path.display());
        let (document, apply_doc_template) = match os_str.to_str().unwrap() {
            "md" => {
                file_name.set_extension("html");
                (Document::from_md(&path, &config.markdown_options)?, true)
            }
            "html" => (Document::from_html(&path)?, true),
            "hbs" => {
                file_name.set_extension("html");
                (
                    Document::from_hbs(&config.global_meta, &path, handlebars)?,
                    false,
                )
            }
            "js" => {
                continue;
            }
            _ => {
                println!("Ignoring {}", path.display());
                continue;
            }
        };

        let html = if apply_doc_template {
            let mut context = PageContext::from_document(&document);
            context.globals = Some(config.global_meta.clone());
            handlebars.render("doc", &context)?
        } else {
            document.html
        };

        let target_path = out_root_folder.join(file_name);
        let fname = filename_to_string(&entry.file_name());
        println!("Writing page {} ({fname})", target_path.display());
        if fname == "index.hbs" {
            println!("INDEX.HTML special case");
            // Just write it plain.
            let mut file = std::fs::File::create(&target_path).context("create_file_as_dir")?;
            file.write_all(html.as_bytes())?;
        } else {
            // Otherwise, get rid of the extension by putting it in a subdirectory.
            util::write_file_as_folder_with_index(&target_path, html, true)?;
        }
    }
    Ok(())
}

fn run() -> anyhow::Result<()> {
    let mut handlebars = handlebars::Handlebars::new();

    handlebars.register_template_file("common_header", "template/common_header.hbs")?;
    handlebars.register_template_file("common_footer", "template/common_footer.hbs")?;
    handlebars.register_template_file("doc", "template/doc.hbs")?;
    handlebars.register_template_file("link_icon", "template/icons/link.hbs")?;
    handlebars.register_template_file("cat_contents", "template/cat_contents.hbs")?;
    handlebars.register_template_file("blog_post", "template/blog_post.hbs")?;
    handlebars.register_template_file("blog_sidebar", "template/blog_sidebar.hbs")?;

    println!("Barebones website generator");

    let mut markdown_options = markdown::Options::gfm();
    markdown_options.compile.allow_dangerous_html = true;
    // println!("md: {:#?}", markdown_options);

    let config = Config {
        indir: PathBuf::from("."),
        outdir: PathBuf::from("build"),
        markdown_options,
        global_meta: GlobalMeta::new()?,
    };

    if !config.outdir.exists() {
        std::fs::create_dir(&config.outdir).context("outdir")?;
    }

    println!("Copying static files...");

    util::copy_recursive(config.indir.join("static"), config.outdir.join("static"))?;
    std::fs::copy(
        config.indir.join("static/img/favicon.ico"),
        config.outdir.join("favicon.ico"),
    )?;

    generate_pages(&config, "src/pages", &mut handlebars)?;

    generate_doctree(&config, "docs", &mut handlebars)?;

    generate_blog(&config, "blog", &mut handlebars)?;
    generate_blog(&config, "news", &mut handlebars)?;

    // OK, we're done - just serve the results.
    let port = 3000;
    println!("Serving on localhost:{}", port);

    Ok(())
}

#[derive(StructOpt, Debug)]
struct Opt {
    #[structopt(short, long, default_value = "3000")]
    port: i32,
}

#[tokio::main]
async fn main() {
    let opt = Opt::from_args();

    run().unwrap();

    server::run_server(opt.port as u16).await;
}
