def page_count(total_items, per_page):
    """Number of pages needed to show total_items, per_page at a time."""
    return total_items // per_page
