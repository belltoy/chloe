import unittest

from pager import page_count


class PageCount(unittest.TestCase):
    def test_exact(self):
        self.assertEqual(page_count(20, 10), 2)

    def test_partial_last_page(self):
        self.assertEqual(page_count(21, 10), 3)

    def test_fewer_than_one_page(self):
        self.assertEqual(page_count(3, 10), 1)


if __name__ == "__main__":
    unittest.main()
