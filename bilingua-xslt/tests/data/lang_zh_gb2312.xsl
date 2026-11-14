<?xml version="1.0" encoding="GB2312"?>
<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
  <xsl:template match="/">
    <html lang="en">
      <body>
        <h1>欢迎光临面包店</h1>
        <p>您的订单有 <b>已到达</b> <xsl:value-of select="count(item)"/> 件商品.</p>
        <img src="{@src}" alt="新鲜面包照片"/>
        <xsl:if test="a"><xsl:text>保存并关闭</xsl:text></xsl:if>
        <p translate="no">ACME-42</p>
      </body>
    </html>
  </xsl:template>
</xsl:stylesheet>
