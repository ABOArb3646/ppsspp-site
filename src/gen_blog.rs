use crate::{config::*, document::*, feed, util};
use std::{cmp::Ordering, path::PathBuf};

// Posts should be passed-in in reverse time order.
fn generate_blog_sidebar(
    title: &str,
    url: &str,
    all_posts: &[Document],
    handlebars: &mut handlebars::Handlebars<'_>,
) -> anyhow::Result<String> {
    let context = SidebarContext {
        title: title.to_string(),
        links: all_posts
            .iter()
            .map(|doc| doc.to_doclink(url))
            .collect::<Vec<_>>(),
    };

    let output = handlebars.render("blog_sidebar", &context)?;
    Ok(output)
}

pub fn generate_blog(
    config: &Config,
    folder: &str,
    title: &str,
    handlebars: &mut handlebars::Handlebars<'_>,
) -> anyhow::Result<Vec<Document>> {
    // For the blog

    let root_folder = config.in_dir.join(folder);
    anyhow::ensure!(root_folder.exists());
    let out_root_folder = config.out_dir.join(folder);

    util::create_folder_if_missing(&out_root_folder)?;

    let mut documents = vec![];

    let listing = root_folder.read_dir()?;

    let mut tag_lookup = std::collections::HashMap::<String, Tag>::new();

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

        let mut doc = Document::from_md(&root_folder.join(entry.file_name()), config)?;

        let [year, month, day, remainder] = parts;
        doc.meta.date = format!("{}-{}-{}", year, month, day);
        if doc.meta.slug.is_empty() {
            println!(
                "Warning: Blog entry missing slug, auto-detecting {}: {}",
                name, remainder
            );
            doc.meta.slug = remainder.to_string();
        }
        assert!(!doc.meta.slug.is_empty());
        doc.meta.url = format!("/{folder}/{}", &doc.meta.slug);
        doc.path = out_root_folder.join(&doc.meta.slug);

        for tag in &doc.meta.tags {
            tag_lookup
                .entry(tag.clone())
                .or_insert_with(|| Tag {
                    name: tag.clone(),
                    articles: vec![],
                })
                .articles
                .push(doc.to_doclink(""));
        }

        documents.push(doc);
    }

    documents.sort_by(|a, b| {
        a.meta
            .date
            .partial_cmp(&b.meta.date)
            .unwrap_or(Ordering::Equal)
            .reverse()
    });

    // Reformat the tag data to a vector.
    let tags = tag_lookup.values().cloned().collect::<Vec<_>>();

    // Add next/forward links
    // for [prev, cur, next] in documents.
    for i in 0..documents.len() {
        if let Some(prev) = documents.get(i.wrapping_sub(1)) {
            documents[i].meta.prev = Some(prev.to_doclink(""));
        }
        if let Some(next) = documents.get(i.wrapping_add(1)) {
            documents[i].meta.next = Some(next.to_doclink(""));
        }
    }

    for doc in &documents {
        let context = PageContext::from_document(doc, &config.global_meta);

        // First, render the blog post itself, without the surrounding chrome. This is so that we can add on
        // more blog posts underneath later for a more continuous experience.
        let post_html = handlebars.render("blog_post", &context)?;
        let sidebar = generate_blog_sidebar(title, &doc.meta.url, &documents, handlebars)?;

        let mut context = PageContext::from_document(doc, &config.global_meta);
        // Now, use that as contents and render into a doc template.
        context.contents = Some(post_html);
        context.sidebar = Some(sidebar);
        //println!("{:#?}", context.meta);
        let html = context.render("doc", handlebars)?;

        let target_path = &doc.path;
        util::write_file_as_folder_with_index(target_path, html, false)?;
    }

    // Generate RSS feed
    feed::write_feed(
        config,
        title,
        "PPSSPP Blog",
        folder,
        &documents,
        feed::FeedFormat::Atom,
        handlebars,
    )?;
    feed::write_feed(
        config,
        title,
        "PPSSPP Blog",
        folder,
        &documents,
        feed::FeedFormat::RSS,
        handlebars,
    )?;

    // Generate the root blog post.
    let sidebar = generate_blog_sidebar(title, &format!("/{}", folder), &documents, handlebars)?;
    // First, render the blog post itself, without the surrounding chrome. This is so that we can add on
    // more blog posts underneath later for a more continuous experience.
    let post_html = documents
        .iter()
        .map(|doc| {
            let context = PageContext::from_document(doc, &config.global_meta);
            // Now, use that as contents and render into a doc template.
            context.render("blog_post", handlebars).unwrap()
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut context =
        PageContext::new(Some(title.to_owned()), Some(post_html), &config.global_meta);
    context.sidebar = Some(sidebar);
    context.tags = tags;
    context.meta = Some(documents[0].meta.clone());

    let html = context.render("doc", handlebars)?;

    let target_path = out_root_folder;
    util::write_file_as_folder_with_index(&target_path, html, false)?;

    println!("Wrote blog {}", folder);

    Ok(documents)
}