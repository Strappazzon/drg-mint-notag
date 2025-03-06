# Contribution Guidelines

By participating, you agree to abide by the [Code of Conduct](https://github.com/Strappazzon/.github/blob/-/CODE_OF_CONDUCT.md).  
When submitting changes to this repository, you agree that the content you contribute may be provided under the terms of the [MIT License](https://opensource.org/licenses/MIT).

## I Have a Question

See [Support Policy](../SUPPORT.md).

If you still need to ask a question and need clarification you can [create a discussion](https://github.com/Strappazzon/drg-mint-notag/discussions/new?category=questions).

## Issue Reporting

> [!CAUTION]
> If you found a security issue, **DO NOT** report it in the issue tracker or elsewhere in public.  
> See: [Security Policy](https://github.com/Strappazzon/.github/blob/-/SECURITY.md).

- Take a look at the [issues](https://github.com/Strappazzon/drg-mint-notag/issues) first to make sure your issue/feature hasn't been reported/requested before.  
  If so, engage in the already existing discussion.
- Check whether your issue/feature is already fixed/implemented.
- Issues in languages other than English will be closed and ignored.
- If you are familiar with the languages used in this repo, you are always welcome to fix/implement an issue/feature yourself.
- Add one issue at a time. Do not put multiple issues into one thread.
- When reporting a bug please describe the steps which reproduce the problem.
- When reporting an error include the event log from mint-notag (`%APPDATA%\drg-mod-integration\data\drg-mod-integration.log`).  
  Include the logs in the Issue in a [file attachment](https://help.github.com/en/github/managing-your-work-on-github/file-attachments-on-issues-and-pull-requests)
  or put it in a [PrivateBin](https://privatebin.net/) ([PasteBin](https://pastebin.com/) is fine too) and provide the link to that paste.
- All issues must be properly formatted with Markdown.  
  If you don't know what that is, read [Basic writing and formatting syntax](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax) before submitting an issue.

### DO NOT post "+1" comments

If you don't have any additional info/context to add but would like to indicate that you're affected by the issue, upvote the original issue.  
See: [Add Reactions](https://github.blog/news-insights/product-news/add-reactions-to-pull-requests-issues-and-comments/)

## Getting Started

See [BUILDING.md](../BUILDING.md).

## Submitting Changes

- Make changes on a separate branch, not on the master branch, then send your changes as a
  [Pull Request](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/proposing-changes-to-your-work-with-pull-requests/about-pull-requests).
- When submitting changes, you confirm that your code is licensed under the terms of the [MIT License](https://opensource.org/licenses/MIT).
- Please test your code before you submit changes. Untested code will **not** be merged!
- Make sure your Pull Request is up-to-date with the rest of the codebase.

## Git Commit Messages

- Use the present tense ("Add feature" not "Added feature").
- Use the imperative mood ("Move cursor to..." not "Moves cursor to...").
- Wrap all lines at 70 columns (except for long URLs).
- Writing a detailed description is not mandatory.
- Reference issues and Pull Requests after the description, if any.  
  Use `Closes:` or `Fixes:`.

### Sample Complete Commit Message

```plaintext
explain the commit in one line

The body of the commit message should explain things in more detail.
Please word-wrap to keep columns to 70 characters or less.

Fixes: https://github.com/Strappazzon/drg-mint-notag/issues/1771
```
