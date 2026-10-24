from pager import page_count


def test_exact():
    assert page_count(20, 10) == 2


def test_partial_last_page():
    assert page_count(21, 10) == 3


def test_fewer_than_one_page():
    assert page_count(3, 10) == 1
