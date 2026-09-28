// Orders.cs: records, LINQ, verbatim strings and pattern matching.
using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;

namespace Shop.Orders
{
    /// <summary>A line on an order.</summary>
    public record OrderLine(string Sku, int Quantity, decimal Price);

    [Serializable]
    [Obsolete("Use OrderService instead", false)]
    public sealed class Order : IComparable<Order>
    {
        private readonly List<OrderLine> _lines = new();
        private static readonly TimeSpan Timeout = TimeSpan.FromSeconds(1.5);
        public const double TaxRate = 0.13d;
        public const float Discount = .25f;
        public const decimal Fee = 2.50m;
        public const long Limit = 0xFFFF_FFFFL;
        public const ulong Mask = 0b1111_0000UL;

        public Guid Id { get; init; } = Guid.NewGuid();
        public string? Note { get; set; }
        public int Count => _lines.Count;

        public event EventHandler<OrderLine>? LineAdded;

        public void Add(OrderLine line)
        {
            if (line is null)
            {
                throw new ArgumentNullException(nameof(line));
            }
            _lines.Add(line);
            LineAdded?.Invoke(this, line);
        }

        public decimal Total()
        {
            decimal total = 0m;
            foreach (var line in _lines)
            {
                total += line.Price * line.Quantity;
            }
            return total * (1 + (decimal)TaxRate);
        }

        public int CompareTo(Order? other) =>
            other is null ? 1 : Total().CompareTo(other.Total());

        public IEnumerable<string> Skus(int minimum)
        {
            return from line in _lines
                   where line.Quantity >= minimum
                   orderby line.Sku descending
                   select line.Sku;
        }

        public string Describe(object value)
        {
            switch (value)
            {
                case int n when n > 100:
                    return "large";
                case string s:
                    return $"text {s.Length} chars";
                default:
                    break;
            }
            return value switch
            {
                null => "nothing",
                decimal d => d.ToString("C"),
                _ => value.ToString() ?? string.Empty,
            };
        }
    }

    public interface IOrderStore<T> where T : class
    {
        Task<T?> FindAsync(Guid id);
    }

    public static class OrderService
    {
        private const string Query = @"SELECT *
FROM ""Orders""
WHERE Id = @id";

        public static async Task<int> SaveAsync(Order order, IOrderStore<Order> store)
        {
            var path = @"C:\orders\" + order.Id + ".json";
            var escaped = "tab\there \"quoted\" \\ done";
            char separator = ',', quote = '\'';
            var @class = "keyword as name";
            int[] counts = { 1, 2, 3 };
            Func<int, int> square = x => x * x;
            Action<string> log = message =>
            {
                Console.WriteLine($"[{DateTime.Now:HH:mm}] {message}");
            };
            unchecked
            {
                counts[0] = (int)(counts[0] * 2654435761u);
            }
            var found = await store.FindAsync(order.Id);
            log(path + escaped + separator + quote + @class + Query);
            return found?.Count ?? square(counts.Length);
        }
    }
}

#region Helpers
#if DEBUG
internal static class Debug { }
#endif
#endregion
